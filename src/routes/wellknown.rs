#![allow(clippy::items_after_statements)]

use axum::{debug_handler, extract::State, response::Json};
use axum_extra::TypedHeader;
use headers::Host;
use std::sync::Arc;

use crate::tenant::config::TenantConfig;
use crate::AppState;

#[debug_handler]
pub async fn wellknown_handler(
    State(state): State<AppState>,
    TypedHeader(host): TypedHeader<Host>,
) -> Json<Arc<TenantConfig>> {
    let hostname = host.hostname();
    let subdomain = crate::tenant::TenantManager::extract_subdomain(hostname);
    let config = state.tenant_manager.get_config(&subdomain).await;
    Json(config)
}
