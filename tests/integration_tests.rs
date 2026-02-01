use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
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
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/repo/src/lib.rs@L42"));
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
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/project/lib/file.rb@L12"));
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
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
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
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/repo/file.rs@L10"));
    // Output should always have https:// prefix regardless of input format
    assert!(location.contains("remote=https://github.com/owner/repo"));
}

#[tokio::test]
async fn test_csp_header_present() {
    // Verify Content-Security-Policy header is set on responses
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let csp = response
        .headers()
        .get("content-security-policy")
        .expect("CSP header should be present");

    let csp_str = csp.to_str().unwrap();
    assert!(
        csp_str.contains("script-src"),
        "CSP should include script-src"
    );
    assert!(
        csp_str.contains("sha256-"),
        "CSP should include script hashes"
    );
    assert!(
        csp_str.contains("style-src"),
        "CSP should include style-src"
    );
    assert!(
        !csp_str.contains("unsafe-inline"),
        "CSP should not use unsafe-inline"
    );
    assert!(
        csp_str.contains("object-src 'none'"),
        "CSP should block plugins"
    );
    assert!(
        csp_str.contains("frame-ancestors 'none'"),
        "CSP should prevent clickjacking"
    );
}

#[tokio::test]
async fn test_javascript_url_not_in_href() {
    // Security: Verify javascript: URLs are not rendered in href attributes
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=javascript:alert('xss')")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Should NOT contain javascript: in any href
    assert!(
        !html.contains("href=\"javascript:"),
        "XSS vulnerability: javascript: URL found in href"
    );
}

#[tokio::test]
async fn test_data_url_not_in_href() {
    // Security: Verify data: URLs are not rendered in href attributes
    use http_body_util::BodyExt;

    let app = create_test_app();
    // URL-encode the data: URL to make it valid in HTTP URI
    let response = app
        .oneshot(
            Request::builder()
                .uri("/?remote=data:text/html,test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Should NOT contain data: in any href
    assert!(
        !html.contains("href=\"data:"),
        "XSS vulnerability: data: URL found in href"
    );
}

#[tokio::test]
async fn test_branch_with_special_chars_is_url_encoded() {
    // Test that branch names with +, #, = are properly URL-encoded in mirror page output
    // Examples: "inputprocessing/c++" and "#pr470" from real GitHub repos
    use http_body_util::BodyExt;

    let app = create_test_app();

    // Mirror path with branch containing + character
    // This tests that render_mirror_page URL-encodes the branch in the srcuri:// URL
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/src/file.rs@L42?branch=feature/c%2B%2B")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // The srcuri:// URL in the HTML should have the branch URL-encoded
    // "feature/c++" becomes "feature%2Fc%2B%2B"
    assert!(
        html.contains("branch=feature%2Fc%2B%2B"),
        "Expected URL-encoded branch in srcuri:// URL. HTML snippet: {}",
        &html[..500.min(html.len())]
    );
}

// Security validation tests

#[tokio::test]
async fn test_invalid_branch_shell_metachar_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Semicolon is a shell metacharacter - should be rejected
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L10?branch=main;rm%20-rf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK); // Error page returns 200
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid branch name"));
}

#[tokio::test]
async fn test_invalid_branch_path_traversal_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L10?branch=../../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid branch name"));
}

#[tokio::test]
async fn test_invalid_remote_shell_metachar_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L10?remote=github.com/owner/repo;whoami")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid remote URL"));
}

#[tokio::test]
async fn test_invalid_remote_path_traversal_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L10?remote=github.com/../../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid remote URL"));
}

#[tokio::test]
async fn test_invalid_workspace_shell_metachar_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Workspace with backtick should be rejected
    let response = app
        .oneshot(
            Request::builder()
                .uri("/my%60repo/file.rs@L10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid workspace name"));
}

#[tokio::test]
async fn test_valid_branch_accepted() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L10?branch=feature/add-tests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    // Should NOT contain error message
    assert!(!html.contains("Invalid"));
    // Should contain the srcuri URL
    assert!(html.contains("srcuri://"));
}

fn create_test_app() -> axum::Router {
    use axum::routing::get;
    use std::path::PathBuf;
    use std::sync::Arc;

    let tenants_dir = PathBuf::from("tenants");
    let tenant_manager = Arc::new(sorcery_server::tenant::TenantManager::new(tenants_dir));
    let base_domain = "srcuri.com".to_string();

    let state = sorcery_server::AppState {
        tenant_manager,
        base_domain,
    };

    axum::Router::new()
        .route("/", get(sorcery_server::routes::root_handler))
        .route("/open", get(sorcery_server::routes::open_handler))
        .route(
            "/.well-known/srcuri.json",
            get(sorcery_server::routes::wellknown_handler),
        )
        .route("/health", get(|| async { "OK" }))
        .fallback(get(sorcery_server::routes::catchall_handler))
        .with_state(state)
        .layer(axum::middleware::from_fn(
            sorcery_server::csp::csp_middleware,
        ))
}

#[tokio::test]
async fn test_file_path_script_injection_escaped() {
    // Security: Verify that <script> tags in file paths are HTML-escaped
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Percent-encoded <script>alert(1)</script>
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/%3Cscript%3Ealert(1)%3C%2Fscript%3E")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Should NOT contain unescaped script tags
    assert!(
        !html.contains("<script>alert"),
        "XSS vulnerability: unescaped script tag in file_path. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_file_path_quotes_escaped() {
    // Security: Verify that quotes in file paths are HTML-escaped
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Percent-encoded " onmouseover="alert(1)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file%22%20onmouseover%3D%22alert(1)")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Should NOT contain unescaped quotes that could break out of attributes
    assert!(
        !html.contains("\" onmouseover="),
        "XSS vulnerability: unescaped quotes in file_path. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_file_path_question_mark_rejected() {
    // Security: ? in file paths is rejected (potential URL injection)
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Percent-encoded file.rs?evil=param
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs%3Fevil%3Dparam")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_line_number_non_numeric_becomes_filename() {
    // Non-numeric ":abc" suffix is treated as part of filename (correct behavior)
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs:abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Non-numeric suffix becomes part of filename, not extracted as line number
    // This is safe - Askama HTML-escapes the content
    assert!(
        html.contains("file.rs:abc"),
        "Non-numeric suffix should be part of filename"
    );
}

#[tokio::test]
async fn test_line_number_numeric_extracted() {
    // Verify that numeric line suffixes ARE extracted properly
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs@L42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Should have @L42 extracted as line number, separate from filename
    // Uses authority-based format (v1 spec) - workspace IS the authority
    assert!(
        html.contains("srcuri://myrepo/file.rs@L42"),
        "Numeric line suffix should be extracted. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_file_path_traversal_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Path traversal attempt
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_too_long_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Generate path > 1024 chars
    let long_path = format!("/myrepo/{}", "a".repeat(1100));
    let response = app
        .oneshot(
            Request::builder()
                .uri(&long_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_shell_metachar_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Semicolon is a shell metacharacter
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file.rs;rm%20-rf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_parentheses_and_brackets_allowed() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/Project%20(1)/src/file[0].rs@L7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    // Uses authority-based format (v1 spec) - workspace IS the authority
    assert!(
        html.contains("srcuri://myrepo/Project (1)/src/file[0].rs@L7"),
        "Expected path with parentheses/brackets to be accepted. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_file_path_quotes_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Double quote
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file%22.rs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_backtick_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Backtick for command substitution
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file%60whoami%60.rs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_angle_brackets_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Angle brackets (HTML/redirect)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/%3Cscript%3E.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

#[tokio::test]
async fn test_file_path_pipe_rejected() {
    use http_body_util::BodyExt;

    let app = create_test_app();
    // Pipe for shell piping
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/file%7Ccat.rs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Invalid file path"));
}

// =============================================================================
// Authority-based URL mode tests
// Verify that srcuri.com URLs convert correctly to srcuri:// protocol URLs
// =============================================================================

#[tokio::test]
async fn test_implicit_workspace_mode() {
    // srcuri.com/myrepo/file.rs@L42 → srcuri://myrepo/file.rs@L42
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/src/main.rs@L42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://myrepo/src/main.rs@L42"),
        "Implicit workspace should produce srcuri://myrepo/... URL. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_explicit_workspace_mode() {
    // srcuri.com/wks/myrepo/file.rs@L42 → srcuri://wks/myrepo/file.rs@L42
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/wks/myrepo/src/main.rs@L42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://wks/myrepo/src/main.rs@L42"),
        "Explicit workspace should produce srcuri://wks/... URL. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_rel_mode() {
    // srcuri.com/rel/file.rs@L42 → srcuri://rel/file.rs@L42
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/rel/src/utils.py@L10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://rel/src/utils.py@L10"),
        "Rel mode should produce srcuri://rel/... URL. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_any_mode() {
    // srcuri.com/any/file.rs@L42 → srcuri://any/file.rs@L42
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/any/src/utils.py@L10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://any/src/utils.py@L10"),
        "Any mode should produce srcuri://any/... URL. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_abs_mode() {
    // srcuri.com/abs/etc/hosts@L1 → srcuri://abs/etc/hosts@L1
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/abs/etc/hosts@L1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://abs/etc/hosts@L1"),
        "Abs mode should produce srcuri://abs/... URL. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_abs_mode_windows_path() {
    // srcuri.com/abs/C:/Users/file.txt@L10 → srcuri://abs/C:/Users/file.txt@L10
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/abs/C:/Users/alice/code/file.txt@L10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://abs/C:/Users/alice/code/file.txt@L10"),
        "Abs mode with Windows path should preserve drive letter. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_ext_mode_github() {
    // srcuri.com/ext/https/github.com/... → provider interstitial page
    // This page uses client-side JS to parse the URL (preserving hash fragments)
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ext/https/github.com/owner/repo/blob/main/src/lib.rs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Provider page has specific elements for client-side URL parsing
    assert!(
        html.contains("Opening in Editor"),
        "Ext mode should serve provider interstitial page with 'Opening in Editor' status. HTML: {}",
        &html[..1000.min(html.len())]
    );
    assert!(
        html.contains("parseRemoteUrl"),
        "Provider page should contain client-side URL parser"
    );
}

#[tokio::test]
async fn test_reserved_workspace_name_rejected() {
    // 'wks', 'rel', 'abs', 'ext' cannot be used as workspace names
    use http_body_util::BodyExt;

    let app = create_test_app();

    // Try using 'rel' as a workspace name (not as mode)
    // /wks/rel/file.rs tries to use 'rel' as workspace name - should be rejected
    let response = app
        .oneshot(
            Request::builder()
                .uri("/wks/rel/file.rs@L1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("Invalid workspace"),
        "Reserved authority 'rel' used as workspace name should be rejected. HTML: {}",
        &html[..1000.min(html.len())]
    );
}

#[tokio::test]
async fn test_implicit_workspace_with_query_params() {
    // srcuri.com/myrepo/file.rs@L42?branch=main → srcuri://myrepo/file.rs@L42?branch=main
    use http_body_util::BodyExt;

    let app = create_test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/myrepo/src/main.rs@L42?branch=develop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("srcuri://myrepo/src/main.rs@L42") && html.contains("branch=develop"),
        "Query params should be preserved. HTML: {}",
        &html[..1000.min(html.len())]
    );
}
