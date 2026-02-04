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
    #[allow(clippy::option_if_let_else)]
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
