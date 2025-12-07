pub mod config;

use config::TenantConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TenantManager {
    configs: Arc<RwLock<HashMap<String, TenantConfig>>>,
    tenants_dir: PathBuf,
}

impl TenantManager {
    pub fn new(tenants_dir: PathBuf) -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            tenants_dir,
        }
    }

    pub async fn get_config(&self, subdomain: &str) -> TenantConfig {
        {
            let configs = self.configs.read().await;
            if let Some(config) = configs.get(subdomain) {
                return config.clone();
            }
        }

        let mut configs = self.configs.write().await;
        if let Some(config) = configs.get(subdomain) {
            return config.clone();
        }

        let config_path = self.tenants_dir.join(format!("{}.json", subdomain));
        let config = TenantConfig::load_from_file(config_path)
            .unwrap_or_else(|_| TenantConfig::default_config());
        configs.insert(subdomain.to_string(), config.clone());
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
