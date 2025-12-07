use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub name: String,
    pub default_remote: Option<String>,
    pub allowed_remotes: Option<Vec<String>>,
}

impl TenantConfig {
    pub fn load_from_file(path: PathBuf) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }

    pub fn default_config() -> Self {
        Self {
            name: "default".to_string(),
            default_remote: None,
            allowed_remotes: None,
        }
    }
}
