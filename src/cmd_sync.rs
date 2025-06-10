use crate::cmd_pull::fetch_feedbin_config_with_tags;
use crate::config_file::{config_path, load_local_config, save_config};
use anyhow::Result;
use std::collections::BTreeMap;

pub async fn run() -> Result<()> {
    let existing = load_local_config().await?.unwrap_or_default();
    let (mut pulled, _tags) = fetch_feedbin_config_with_tags(Some(&existing.vars)).await?;

    let mut vars: BTreeMap<String, String> = existing
        .vars
        .into_iter()
        .filter(|(k, _)| !pulled.vars.contains_key(k))
        .collect();
    vars.extend(pulled.vars.into_iter());

    pulled.vars = vars;
    save_config(&pulled).await?;
    println!("Wrote {}", config_path()?.display());
    Ok(())
}
