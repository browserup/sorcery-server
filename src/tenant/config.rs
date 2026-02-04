use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub name: String,
    pub default_remote: Option<String>,
    pub allowed_remotes: Option<Vec<String>>,
}

impl TenantConfig {
    /// # Errors
    /// Returns an error if the file cannot be read or contains invalid JSON.
    pub async fn load_from_file(path: PathBuf) -> Result<Self, std::io::Error> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    #[must_use]
    pub fn default_config() -> Self {
        Self {
            name: "default".to_string(),
            default_remote: None,
            allowed_remotes: None,
        }
    }
}
