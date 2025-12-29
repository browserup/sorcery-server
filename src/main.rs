use axum::{
    routing::get,
    Router,
    response::{Response, Redirect, IntoResponse},
    body::Body,
    http::{StatusCode, header, Uri},
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

#[tokio::main]
async fn main() {
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
            .expect("Failed to build rate limiter config")
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

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!("\n  Sorcery Server running!\n");
    println!("   Base URL:     http://localhost:{}", port);
    println!("   Provider:     http://localhost:{}/github.com/owner/repo/blob/main/file.rs#L42", port);
    println!("   Mirror:       http://localhost:{}/repo/src/lib.rs:42?branch=main", port);
    println!("   Health:       http://localhost:{}/health\n", port);

    if let Err(e) = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
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
    let host_without_port = host.split(':').next().unwrap_or(&host);
    let is_localhost = host_without_port == "localhost" || host_without_port == "127.0.0.1";

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript");

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(86400))
            .unwrap_or(SystemTime::now());
        let expires_http = HttpDate::from(expires_time).to_string();
        builder = builder
            .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
            .header(header::EXPIRES, expires_http);
    }

    builder.body(Body::from(content)).unwrap()
}

const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#9333ea"/><stop offset="100%" stop-color="#c026d3"/></linearGradient></defs><rect x="4" y="22" width="13" height="3" rx="1" transform="rotate(-45 4 22)" fill="#1a1a1a"/><path d="M21 4l1.3 5.7 5.7 1.3-5.7 1.3L21 18l-1.3-5.7L14 11l5.7-1.3z" fill="url(#g)"/></svg>"##;

async fn serve_favicon(Host(host): Host) -> Response<Body> {
    let host_without_port = host.split(':').next().unwrap_or(&host);
    let is_localhost = host_without_port == "localhost" || host_without_port == "127.0.0.1";

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml");

    if !is_localhost {
        let expires_time = SystemTime::now()
            .checked_add(Duration::from_secs(7776000))
            .unwrap_or(SystemTime::now());
        let expires_http = HttpDate::from(expires_time).to_string();
        builder = builder
            .header(header::CACHE_CONTROL, "public, max-age=7776000, immutable")
            .header(header::EXPIRES, expires_http);
    }

    builder.body(Body::from(FAVICON_SVG)).unwrap()
}

async fn serve_favicon_svg(Host(host): Host) -> Response<Body> {
    serve_favicon(Host(host)).await
}
