use axum::response::Html;

pub async fn provider_handler() -> Html<&'static str> {
    Html(include_str!("../templates/provider.html"))
}
