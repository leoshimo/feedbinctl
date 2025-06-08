# README.md

Manage Feedbin [Saved Searches](https://github.com/feedbin/feedbin-api/blob/master/content/saved-searches.md) from a configuration file.

## Commands

* `feedbinctl pull` - pull current Saved Searches and tags; writes tag‑ID variables into your config.
* `feedbinctl diff` - show differences between your configuration file and Feedbin.
* `feedbinctl push` - make Feedbin match the configuration file.
* `feedbinctl auth login` -store your Feedbin token securely in the OS keyring.

## Installation

```sh
cargo install feedbinctl
```

## Configuration file

Default location: `~/.config/feedbinctl/config`

```toml
# feedbinctl configuration

[vars]
github_tag_id = 42           # GitHub
acme_tag_id = 99             # Acme
x_tag_id = 1234              # X
mastadon_tag_id = 54321      # X
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
```

## Example workflow

```sh
# Bootstrap config with current Feedbin state
feedbinctl pull > ~/.config/feedbinctl/config

# Edit desired searches
emacs ~/.config/feedbinctl/config

# Preview changes
feedbinctl diff

# Apply when ready
feedbinctl push
```
