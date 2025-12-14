use axum::response::Html;

pub async fn open_handler() -> Html<&'static str> {
    Html(include_str!("../templates/open.html"))
}
