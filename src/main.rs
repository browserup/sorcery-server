use anyhow::Context;
use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use axum_extra::TypedHeader;
use headers::Host;
use httpdate::HttpDate;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::{KeyExtractor, PeerIpKeyExtractor, SmartIpKeyExtractor},
    GovernorError, GovernorLayer,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use sorcery_server::{
    csp, routes,
    subdomain::{self, SubdomainMode},
    tenant, AppState,
};

const ONE_DAY_SECS: u64 = 86_400;
const NINETY_DAYS_SECS: u64 = 7_776_000;

#[derive(Debug, Clone, Copy)]
struct ConfigurableIpExtractor {
    trust_proxy: bool,
}

impl KeyExtractor for ConfigurableIpExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        if self.trust_proxy {
            SmartIpKeyExtractor.extract(req)
        } else {
            PeerIpKeyExtractor.extract(req)
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        tracing::error!("{:#}", err);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sorcery_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let tenants_dir = std::env::var("TENANTS_DIR")
        .map_or_else(|_| PathBuf::from("sorcery-server/tenants"), PathBuf::from);

    let base_domain = std::env::var("BASE_DOMAIN").unwrap_or_else(|_| "srcuri.com".to_string());

    let tenant_manager = Arc::new(tenant::TenantManager::new(tenants_dir));

    let state = AppState {
        tenant_manager,
        base_domain,
    };

    let trust_proxy = std::env::var("TRUST_PROXY_HEADERS")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let governor_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(1)
            .burst_size(60)
            .key_extractor(ConfigurableIpExtractor { trust_proxy })
            .finish()
            .context("build rate limiter config")?,
    );

    let app = Router::new()
        // Health check available on all subdomains (not rate limited)
        .route("/health", get(health_handler))
        // Direct protocol routes
        .route("/", get(subdomain_aware_root))
        .route("/open", get(routes::open_handler))
        .route("/.well-known/srcuri.json", get(routes::wellknown_handler))
        .route("/static/app.js", get(serve_app_js))
        .route("/favicon.ico", get(serve_favicon))
        .route("/favicon.svg", get(serve_favicon_svg))
        .fallback(get(subdomain_aware_fallback))
        .with_state(state)
        .layer(axum::middleware::from_fn(csp::csp_middleware))
        .layer(GovernorLayer::new(governor_config))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([axum::http::Method::GET])
                .allow_headers([header::ACCEPT, header::CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let host: std::net::IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|h| h.parse().ok())
        .unwrap_or_else(|| [0, 0, 0, 0].into());

    let addr = SocketAddr::from((host, port));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind to {addr}"))?;

    tracing::info!("Sorcery Server listening on {}", addr);
    tracing::info!("Base URL: http://{}", addr);
    tracing::info!(
        "Provider: http://{}/github.com/owner/repo/blob/main/file.rs#L42",
        addr
    );
    tracing::info!("Mirror: http://{}/repo/src/lib.rs:42?branch=main", addr);
    tracing::info!("Health: http://{}/health", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("server error")?;

    Ok(())
}

async fn subdomain_aware_root(
    axum::extract::State(state): axum::extract::State<AppState>,
    TypedHeader(host): TypedHeader<Host>,
    uri: Uri,
    query: Query<routes::passthrough::PassthroughQuery>,
) -> Response<Body> {
    let host = host.hostname().to_string();
    let mode = subdomain::detect_mode(&host, &uri);
    match mode {
        SubdomainMode::WwwRedirect => {
            let new_uri = format!(
                "https://{}{}",
                state.base_domain,
                uri.path_and_query()
                    .map_or("/", axum::http::uri::PathAndQuery::as_str)
            );
            Redirect::permanent(&new_uri).into_response()
        }
        SubdomainMode::GetSorcery => routes::getsorcery_landing().await.into_response(),
        SubdomainMode::DirectProtocol | SubdomainMode::EnterpriseTenant(_) => {
            routes::root_handler(query).await.into_response()
        }
    }
}

async fn subdomain_aware_fallback(
    axum::extract::State(state): axum::extract::State<AppState>,
    TypedHeader(host): TypedHeader<Host>,
    uri: Uri,
    query: axum::extract::Query<routes::passthrough::MirrorQuery>,
) -> Response<Body> {
    let host = host.hostname().to_string();
    let mode = subdomain::detect_mode(&host, &uri);
    match mode {
        SubdomainMode::WwwRedirect => {
            let new_uri = format!(
                "https://{}{}",
                state.base_domain,
                uri.path_and_query()
                    .map_or("/", axum::http::uri::PathAndQuery::as_str)
            );
            Redirect::permanent(&new_uri).into_response()
        }
        SubdomainMode::GetSorcery => match uri.path() {
            "/install.sh" => routes::install_script_handler().await,
            "/chrome" => routes::chrome_redirect_handler().await.into_response(),
            "/support/editors" => routes::getsorcery_editors().await.into_response(),
            "/support/platforms" => routes::getsorcery_platforms().await.into_response(),
            "/support/frameworks" => routes::getsorcery_frameworks().await.into_response(),
            _ => (StatusCode::NOT_FOUND, "Not Found").into_response(),
        },
        SubdomainMode::DirectProtocol | SubdomainMode::EnterpriseTenant(_) => {
            routes::catchall_handler(uri, query).await.into_response()
        }
    }
}

async fn health_handler() -> &'static str {
    "OK"
}

async fn serve_app_js(TypedHeader(host): TypedHeader<Host>) -> Response<Body> {
    let content = include_str!("static/app.js");
    let host = host.hostname();
    let is_localhost = subdomain::is_localhost(host);

    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript"),
    );

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(ONE_DAY_SECS))
            .unwrap_or_else(SystemTime::now);
        let expires_http = HttpDate::from(expires_time).to_string();
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400, immutable"),
        );
        match HeaderValue::from_str(&expires_http) {
            Ok(value) => {
                response.headers_mut().insert(header::EXPIRES, value);
            }
            Err(err) => {
                tracing::warn!(error = %err, "Failed to set Expires header");
            }
        }
    }

    response
}

const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><polygon points="52,428 87,463 328,230 293,195" fill="#1a1a1a"/><polygon points="370,30 398,117 485,145 398,173 370,260 342,173 255,145 342,117" fill="url(#g)"/><polygon points="370,125 375,140 390,145 375,150 370,165 365,150 350,145 365,140" fill="white"/><defs><radialGradient id="g" cx="370" cy="145" r="115" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#9333ea"/><stop offset="70%" stop-color="#c026d3"/><stop offset="100%" stop-color="#f59e0b"/></radialGradient></defs></svg>"##;

async fn serve_favicon(TypedHeader(host): TypedHeader<Host>) -> Response<Body> {
    let host = host.hostname();
    let is_localhost = subdomain::is_localhost(host);

    let mut response = Response::new(Body::from(FAVICON_SVG));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(NINETY_DAYS_SECS))
            .unwrap_or_else(SystemTime::now);
        let expires_http = HttpDate::from(expires_time).to_string();
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=7776000, immutable"),
        );
        match HeaderValue::from_str(&expires_http) {
            Ok(value) => {
                response.headers_mut().insert(header::EXPIRES, value);
            }
            Err(err) => {
                tracing::warn!(error = %err, "Failed to set Expires header");
            }
        }
    }

    response
}

async fn serve_favicon_svg(host: TypedHeader<Host>) -> Response<Body> {
    serve_favicon(host).await
}
