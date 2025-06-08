# AGENT.md

Internal design notes for **feedbinctl**

---

## Reference URLs

* Saved Searches API → [https://github.com/feedbin/feedbin-api/blob/master/content/saved-searches.md](https://github.com/feedbin/feedbin-api/blob/master/content/saved-searches.md)
* Feedbin API repo   → [https://github.com/feedbin/feedbin-api](https://github.com/feedbin/feedbin-api)
* Search‑syntax help → [https://feedbin.com/help/saved-searches/](https://feedbin.com/help/saved-searches/)

---

## Open Tasks

### Subcommands

| Command                      | Purpose                                                                                                |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ |
| `pull`                       | Fetch Saved Searches and tags, emit a complete configuration (`[vars]` + `[[searches]]`).              |
| `diff`                       | Resolve variables, fetch current Feedbin state, compute & display create / update / delete operations. |
| `push`                       | Execute the operations from `diff`; supports `--yes` to skip confirmation.                             |
| `auth login` / `auth logout` | Store / remove token in OS keyring; fall back to `FEEDBIN_TOKEN`.                                      |

### Core Functionality

* Parse / serialise configuration (TOML primary; YAML/JSON optional).
* Tag‑name → ID resolution **every run** by calling the Taggings API.
* Operation planner that converts Desired vs Actual into API requests.

---

## Testing

* **API → In‑Memory**: parse real/fixture JSON responses into internal structs.
* **In‑Memory → Config**: serialise structs back to a configuration file and assert round‑trip fidelity.
* **In‑Memory ⟷ In‑Memory diff**: compare two struct sets and verify the list of API operations generated.

Mock HTTP with `wiremock` to keep tests offline.

---

## Recommended Crates

| Area         | Crate                            |
| ------------ | -------------------------------- |
| CLI parsing  | `clap` (v4)                      |
| Async HTTP   | `reqwest` + `tokio`              |
| Config parse | `serde` + `toml_edit`            |
| Templating   | `handlebars`                     |
| Credentials  | `keyring`                        |
| Errors       | `anyhow`, `thiserror`            |
| Logging      | `tracing` + `tracing_subscriber` |
| Testing HTTP | `wiremock`                       |
| XDG paths    | `directories`                    |

## Build

* Always run `cargo check`, `cargo build`, and `cargo test`.
* Fix all warnings and errors until all three commands succeed.
