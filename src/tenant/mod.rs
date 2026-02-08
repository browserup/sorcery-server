pub mod config;

use config::TenantConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

#[derive(Debug)]
pub struct TenantManager {
    configs: RwLock<HashMap<String, Arc<TenantConfig>>>,
    tenants_dir: PathBuf,
}

fn is_valid_subdomain(s: &str) -> bool {
    let len = s.len();
    if len == 0 || len > 63 {
        return false;
    }
    if s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl TenantManager {
    #[must_use]
    pub fn new(tenants_dir: PathBuf) -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            tenants_dir,
        }
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn get_config(&self, subdomain: &str) -> Arc<TenantConfig> {
        let cached = self.configs.read().await.get(subdomain).cloned();
        if let Some(config) = cached {
            return config;
        }

        let config_path = self.tenants_dir.join(format!("{subdomain}.json"));
        let config = match TenantConfig::load_from_file(config_path).await {
            Ok(config) => config,
            Err(err) => {
                warn!(error = %err, subdomain = %subdomain, "Falling back to default tenant config");
                TenantConfig::default_config()
            }
        };
        let config = Arc::new(config);

        let mut configs = self.configs.write().await;
        let entry = configs
            .entry(subdomain.to_string())
            .or_insert_with(|| Arc::clone(&config));
        Arc::clone(entry)
    }

    #[must_use]
    pub fn extract_subdomain(host: &str) -> String {
        let host_without_port = host.split(':').next().unwrap_or(host);

        match host_without_port.split('.').next() {
            Some("srcuri") => "default".to_string(),
            Some(sub) if is_valid_subdomain(sub) => sub.to_string(),
            _ => "default".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_subdomains() {
        assert_eq!(TenantManager::extract_subdomain("acme.srcuri.com"), "acme");
        assert_eq!(
            TenantManager::extract_subdomain("my-tenant.srcuri.com"),
            "my-tenant"
        );
        assert_eq!(TenantManager::extract_subdomain("a1b2.srcuri.com"), "a1b2");
    }

    #[test]
    fn srcuri_returns_default() {
        assert_eq!(TenantManager::extract_subdomain("srcuri.com"), "default");
    }

    #[test]
    fn path_traversal_returns_default() {
        assert_eq!(
            TenantManager::extract_subdomain("../etc.srcuri.com"),
            "default"
        );
        assert_eq!(
            TenantManager::extract_subdomain("foo/bar.srcuri.com"),
            "default"
        );
        assert_eq!(
            TenantManager::extract_subdomain("..%2fetc.srcuri.com"),
            "default"
        );
    }

    #[test]
    fn uppercase_returns_default() {
        assert_eq!(
            TenantManager::extract_subdomain("ACME.srcuri.com"),
            "default"
        );
        assert_eq!(
            TenantManager::extract_subdomain("Acme.srcuri.com"),
            "default"
        );
    }

    #[test]
    fn empty_returns_default() {
        assert_eq!(TenantManager::extract_subdomain(""), "default");
        assert_eq!(TenantManager::extract_subdomain(".srcuri.com"), "default");
    }

    #[test]
    fn too_long_returns_default() {
        let long = format!("{}.srcuri.com", "a".repeat(64));
        assert_eq!(TenantManager::extract_subdomain(&long), "default");
    }

    #[test]
    fn leading_trailing_hyphen_returns_default() {
        assert_eq!(
            TenantManager::extract_subdomain("-acme.srcuri.com"),
            "default"
        );
        assert_eq!(
            TenantManager::extract_subdomain("acme-.srcuri.com"),
            "default"
        );
    }

    #[test]
    fn port_stripping_works() {
        assert_eq!(
            TenantManager::extract_subdomain("acme.srcuri.com:8080"),
            "acme"
        );
        assert_eq!(
            TenantManager::extract_subdomain("srcuri.com:3000"),
            "default"
        );
    }

    #[test]
    fn bare_hostname_without_dots() {
        assert_eq!(TenantManager::extract_subdomain("localhost"), "localhost");
        assert_eq!(
            TenantManager::extract_subdomain("localhost:3000"),
            "localhost"
        );
    }
}
