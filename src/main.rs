use anyhow::Context;
use axum::{
    routing::get,
    Router,
    response::{Response, Redirect, IntoResponse},
    body::Body,
    http::{StatusCode, header, HeaderValue, Uri},
    extract::{Host, Query},
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use httpdate::HttpDate;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use sorcery_server::{AppState, csp, routes, tenant, subdomain::{self, SubdomainMode}};

const ONE_DAY_SECS: u64 = 86_400;
const NINETY_DAYS_SECS: u64 = 7_776_000;

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
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("sorcery-server/tenants"));

    let base_domain = std::env::var("BASE_DOMAIN")
        .unwrap_or_else(|_| "srcuri.com".to_string());

    let tenant_manager = Arc::new(tenant::TenantManager::new(tenants_dir));

    let state = AppState { tenant_manager, base_domain };

    // Rate limiting: 60 requests per minute per IP (1 request per second on average)
    let governor_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(1)
            .burst_size(60)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .context("build rate limiter config")?
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
        .layer(GovernorLayer { config: governor_config })
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind to {}", addr))?;

    tracing::info!("Sorcery Server running");
    tracing::info!("Base URL: http://localhost:{}", port);
    tracing::info!("Provider: http://localhost:{}/github.com/owner/repo/blob/main/file.rs#L42", port);
    tracing::info!("Mirror: http://localhost:{}/repo/src/lib.rs:42?branch=main", port);
    tracing::info!("Health: http://localhost:{}/health", port);

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("server error")?;

    Ok(())
}

async fn subdomain_aware_root(
    axum::extract::State(state): axum::extract::State<AppState>,
    Host(host): Host,
    uri: Uri,
    query: Query<routes::passthrough::PassthroughQuery>,
) -> Response<Body> {
    let mode = subdomain::detect_mode(&host, &uri);
    match mode {
        SubdomainMode::WwwRedirect => {
            let new_uri = format!("https://{}{}", state.base_domain, uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));
            Redirect::permanent(&new_uri).into_response()
        }
        SubdomainMode::DirectProtocol | SubdomainMode::EnterpriseTenant(_) => {
            routes::root_handler(query).await.into_response()
        }
    }
}

async fn subdomain_aware_fallback(
    axum::extract::State(state): axum::extract::State<AppState>,
    Host(host): Host,
    uri: Uri,
    query: axum::extract::Query<routes::passthrough::MirrorQuery>,
) -> Response<Body> {
    let mode = subdomain::detect_mode(&host, &uri);
    match mode {
        SubdomainMode::WwwRedirect => {
            let new_uri = format!("https://{}{}", state.base_domain, uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));
            Redirect::permanent(&new_uri).into_response()
        }
        SubdomainMode::DirectProtocol | SubdomainMode::EnterpriseTenant(_) => {
            routes::catchall_handler(uri, query).await.into_response()
        }
    }
}

async fn health_handler() -> &'static str {
    "OK"
}

async fn serve_app_js(Host(host): Host) -> Response<Body> {
    let content = include_str!("static/app.js");
    let is_localhost = subdomain::is_localhost(&host);

    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript"),
    );

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(ONE_DAY_SECS))
            .unwrap_or(SystemTime::now());
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

async fn serve_favicon(Host(host): Host) -> Response<Body> {
    let is_localhost = subdomain::is_localhost(&host);

    let mut response = Response::new(Body::from(FAVICON_SVG));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(NINETY_DAYS_SECS))
            .unwrap_or(SystemTime::now());
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

async fn serve_favicon_svg(Host(host): Host) -> Response<Body> {
    serve_favicon(Host(host)).await
}
