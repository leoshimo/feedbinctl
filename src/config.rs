use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    #[serde(default)]
    pub searches: Vec<Search>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Search {
    pub name: String,
    pub query: String,
}

#[cfg(test)]
mod tests {
    use super::*;


    const SAMPLE_CONFIG: &str = r#"
# feedbinctl configuration

[vars]
github_tag_id = "42"           # GitHub
acme_tag_id = "99"             # Acme
x_tag_id = "1234"              # X
mastadon_tag_id = "54321"      # Mastodon
topic_query = '"emacs" OR "plan9" OR "rust"'
recent_filter = 'published:>now-7d'
social = "tag_id:{{ x_tag_id }} OR tag_id:{{ mastodon_tag_id }}"

[[searches]]
name  = "Keywords"
query = "{{ topic_query }}"

[[searches]]
name  = "Keywords Unread"
query = "{{ topic_query }} is:starred"

[[searches]]
name  = "Keywords Recent"
query = "{{ topic_query }} {{ recent_filter }}"

[[searches]]
name  = "Social"
query = "{{ social }}"
"#;
    #[test]
    fn parse_config() {
        let cfg: Config = toml_edit::de::from_str(SAMPLE_CONFIG).unwrap();
        assert_eq!(cfg.vars.get("github_tag_id"), Some(&"42".to_string()));
        assert_eq!(cfg.searches.len(), 4);
        assert_eq!(cfg.searches[0].name, "Keywords");
    }

    #[test]
    fn round_trip_config() {
        let cfg: Config = toml_edit::de::from_str(SAMPLE_CONFIG).unwrap();
        let toml = toml_edit::ser::to_string_pretty(&cfg).unwrap();
        let de: Config = toml_edit::de::from_str(&toml).unwrap();
        assert_eq!(cfg, de);
    }
}
