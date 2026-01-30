use askama::Template;
use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
};

#[derive(Template)]
#[template(path = "getsorcery-landing.html")]
struct LandingTemplate;

#[derive(Template)]
#[template(path = "getsorcery-editors.html")]
struct EditorsTemplate;

#[derive(Template)]
#[template(path = "getsorcery-platforms.html")]
struct PlatformsTemplate;

#[derive(Template)]
#[template(path = "getsorcery-frameworks.html")]
struct FrameworksTemplate;

pub async fn landing_handler() -> impl IntoResponse {
    let template = LandingTemplate;
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "Failed to render getsorcery landing template");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn editors_handler() -> impl IntoResponse {
    let template = EditorsTemplate;
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "Failed to render editors template");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn platforms_handler() -> impl IntoResponse {
    let template = PlatformsTemplate;
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "Failed to render platforms template");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn frameworks_handler() -> impl IntoResponse {
    let template = FrameworksTemplate;
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "Failed to render frameworks template");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn install_script_handler() -> Response<Body> {
    let content = include_str!("../static/install.sh");

    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );

    response
}

pub async fn chrome_redirect_handler() -> impl IntoResponse {
    axum::response::Redirect::temporary("https://chrome.google.com/webstore/detail/sorcery")
}
