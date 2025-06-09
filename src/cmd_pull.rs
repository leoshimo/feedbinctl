use crate::config::{Config, Search};
use anyhow::{Context, Result};
use keyring::Entry;
use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
struct SavedSearch {
    name: String,
    query: String,
}

#[derive(Debug, Deserialize)]
struct Tag {
    id: u64,
    name: String,
}

fn tag_var_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.ends_with('_') {
        out.pop();
    }
    out.push_str("_tag_id");
    out
}

pub async fn fetch_feedbin_config() -> Result<Config> {
    let token = match std::env::var("FEEDBIN_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            let entry =
                Entry::new("feedbinctl", "feedbin").context("failed to open keyring entry")?;
            entry
                .get_password()
                .context("FEEDBIN_TOKEN not set and failed to read credentials from keyring")?
        }
    };
    let (username, password) = token
        .split_once(':')
        .context("FEEDBIN_TOKEN must be in 'username:password' format")?;

    let client = Client::new();

    let searches: Vec<SavedSearch> = client
        .get("https://api.feedbin.com/v2/saved_searches.json")
        .basic_auth(username, Some(password))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("failed to fetch saved searches")?;

    let tags: Vec<Tag> = client
        .get("https://api.feedbin.com/v2/tags.json")
        .basic_auth(username, Some(password))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("failed to fetch tags")?;

    let mut vars = BTreeMap::new();
    for tag in &tags {
        vars.insert(tag_var_name(&tag.name), tag.id.to_string());
    }

    let searches = searches
        .into_iter()
        .map(|s| {
            let mut query = s.query;
            for (name, id) in &vars {
                let needle = format!("tag_id:{id}");
                if query.contains(&needle) {
                    let replacement = format!("tag_id:{{{{ {name} }}}}");
                    query = query.replace(&needle, &replacement);
                }
            }
            Search {
                name: s.name,
                query,
            }
        })
        .collect();

    let config = Config { vars, searches };

    Ok(config)
}

pub async fn run() -> Result<()> {
    let config = fetch_feedbin_config().await?;

    fn sq(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    println!("# feedbinctl configuration\n");
    println!("[vars]");
    for (k, v) in &config.vars {
        println!("{} = \"{}\"", k, v);
    }

    for search in &config.searches {
        println!();
        println!("[[searches]]");
        println!("name = {}", sq(&search.name));
        println!("query = {}", sq(&search.query));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::tag_var_name;

    #[test]
    fn tag_var_name_basic() {
        assert_eq!(tag_var_name("GitHub"), "github_tag_id");
        assert_eq!(tag_var_name("My Tag"), "my_tag_tag_id");
        assert_eq!(tag_var_name("C++"), "c__tag_id");
    }
}
