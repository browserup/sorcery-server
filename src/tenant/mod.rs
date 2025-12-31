pub mod config;

use config::TenantConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct TenantManager {
    configs: RwLock<HashMap<String, Arc<TenantConfig>>>,
    tenants_dir: PathBuf,
}

impl TenantManager {
    pub fn new(tenants_dir: PathBuf) -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            tenants_dir,
        }
    }

    pub async fn get_config(&self, subdomain: &str) -> Arc<TenantConfig> {
        let mut configs = self.configs.write().await;
        if let Some(config) = configs.get(subdomain) {
            return Arc::clone(config);
        }

        let config_path = self.tenants_dir.join(format!("{}.json", subdomain));
        let config = Arc::new(
            TenantConfig::load_from_file(config_path)
                .await
                .unwrap_or_else(|_| TenantConfig::default_config()),
        );
        configs.insert(subdomain.to_string(), Arc::clone(&config));
        config
    }

    pub fn extract_subdomain(host: &str) -> String {
        if let Some(subdomain) = host.split('.').next() {
            if subdomain == "srcuri" || subdomain.contains(':') {
                "default".to_string()
            } else {
                subdomain.to_string()
            }
        } else {
            "default".to_string()
        }
    }
}
