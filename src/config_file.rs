use crate::config::Config;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("feedbinctl").join("config"));
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .context("could not determine home directory")?;
    Ok(home.join(".config").join("feedbinctl").join("config"))
}

pub async fn load_local_config() -> Result<Option<Config>> {
    let path = config_path()?;
    let data = match tokio::fs::read_to_string(&path).await {
        Ok(d) => d,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let cfg: Config = toml_edit::de::from_str(&data).context("failed to parse config file")?;
    Ok(Some(cfg))
}

pub async fn save_config(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let data = toml_edit::ser::to_string_pretty(cfg).context("failed to serialise config")?;
    tokio::fs::write(&path, data)
        .await
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
