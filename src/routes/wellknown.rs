use axum::{
    response::Json,
    extract::State,
    http::HeaderMap,
    debug_handler,
};
use crate::tenant::config::TenantConfig;
use crate::AppState;

#[debug_handler]
pub async fn wellknown_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<TenantConfig> {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or(&state.base_domain);

    let subdomain = crate::tenant::TenantManager::extract_subdomain(host);
    let config = state.tenant_manager.get_config(&subdomain).await;
    Json(config)
}
