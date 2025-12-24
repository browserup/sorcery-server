use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_wellknown_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/srcuri.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_open_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/open").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_root_landing_page() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_passthrough_github_redirect() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=https://github.com/owner/repo/blob/main/src/lib.rs%23L42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("/repo/src/lib.rs:42"));
    assert!(location.contains("branch=main"));
    assert!(location.contains("remote=https://github.com/owner/repo"));
}

#[tokio::test]
async fn test_passthrough_gitlab_redirect() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=https://gitlab.com/group/project/-/blob/master/lib/file.rb%23L12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("/project/lib/file.rb:12"));
    assert!(location.contains("remote=https://gitlab.com/group/project"));
}

#[tokio::test]
async fn test_passthrough_repo_only() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=https://github.com/owner/repo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("/repo?"));
    assert!(location.contains("remote=https://github.com/owner/repo"));
}

#[tokio::test]
async fn test_passthrough_invalid_url() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=not-a-valid-url")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_passthrough_without_https_prefix() {
    // Test that remote= without https:// prefix is normalized to include it
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=github.com/owner/repo/blob/main/file.rs%23L10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("/repo/file.rs:10"));
    // Output should always have https:// prefix regardless of input format
    assert!(location.contains("remote=https://github.com/owner/repo"));
}

fn create_test_app() -> axum::Router {
    use std::path::PathBuf;
    use std::sync::Arc;

    let tenants_dir = PathBuf::from("tenants");
    let tenant_manager = Arc::new(sorcery_server::tenant::TenantManager::new(tenants_dir));
    let base_domain = "srcuri.com".to_string();

    let state = sorcery_server::AppState { tenant_manager, base_domain };

    axum::Router::new()
        .route("/", axum::routing::get(sorcery_server::routes::root_handler))
        .route("/open", axum::routing::get(sorcery_server::routes::open_handler))
        .route(
            "/.well-known/srcuri.json",
            axum::routing::get(sorcery_server::routes::wellknown_handler),
        )
        .route("/health", axum::routing::get(|| async { "OK" }))
        .with_state(state)
}
