// Content Security Policy module
// Hashes are computed at build time by build.rs

// Include the auto-generated hashes
include!(concat!(env!("OUT_DIR"), "/csp_hashes.rs"));

use axum::{
    body::Body,
    http::{header, Request, Response},
    middleware::Next,
};

/// CSP middleware that adds Content-Security-Policy header to all responses
pub async fn csp_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(request).await;

    // Build the CSP header value
    // - script-src: Only allow scripts with matching hashes (computed at build time)
    // - style-src: Allow inline styles (lower risk than scripts)
    // - object-src: Block all plugins (Flash, Java, etc.)
    // - base-uri: Prevent base tag injection
    // - frame-ancestors: Prevent clickjacking
    // - form-action: Only allow forms to submit to same origin
    let csp_value = format!(
        "default-src 'self'; \
         script-src {}; \
         style-src 'self' 'unsafe-inline'; \
         object-src 'none'; \
         base-uri 'self'; \
         frame-ancestors 'none'; \
         form-action 'self'",
        script_src_hashes()
    );

    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        csp_value.parse().unwrap(),
    );

    response
}
