pub mod csp;
pub mod parsing;
pub mod routes;
pub mod subdomain;
pub mod tenant;

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub tenant_manager: Arc<tenant::TenantManager>,
    pub base_domain: String,
}
