use crate::cmd_pull::fetch_feedbin_config;
use crate::config::{Config, Search};
use anyhow::{Context, Result};

use crate::config_file::load_local_config;
use handlebars::Handlebars;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub enum DiffOp {
    Create(Search),
    Update { from: Search, to: Search },
    Delete(Search),
}

fn resolve_searches(cfg: &Config) -> Result<BTreeMap<String, String>> {
    let hb = Handlebars::new();
    let mut out = BTreeMap::new();
    for s in &cfg.searches {
        let rendered = hb
            .render_template(&s.query, &cfg.vars)
            .with_context(|| format!("failed to render query for '{}'", s.name))?;
        out.insert(s.name.clone(), rendered);
    }
    Ok(out)
}

pub fn diff_configs(current: &Config, desired: &Config) -> Result<Vec<DiffOp>> {
    let cur = resolve_searches(current)?;
    let des = resolve_searches(desired)?;

    let mut ops = Vec::new();

    for (name, new_q) in &des {
        match cur.get(name) {
            None => ops.push(DiffOp::Create(Search {
                name: name.clone(),
                query: new_q.clone(),
            })),
            Some(old_q) => {
                if old_q != new_q {
                    ops.push(DiffOp::Update {
                        from: Search {
                            name: name.clone(),
                            query: old_q.clone(),
                        },
                        to: Search {
                            name: name.clone(),
                            query: new_q.clone(),
                        },
                    });
                }
            }
        }
    }

    for (name, old_q) in &cur {
        if !des.contains_key(name) {
            ops.push(DiffOp::Delete(Search {
                name: name.clone(),
                query: old_q.clone(),
            }));
        }
    }

    Ok(ops)
}

pub async fn run() -> Result<()> {
    let desired = match load_local_config().await? {
        Some(cfg) => cfg,
        None => return Ok(()),
    };
    let current = fetch_feedbin_config(None).await?;

    let ops = diff_configs(&current, &desired)?;

    let desired_raw: BTreeMap<_, _> = desired
        .searches
        .iter()
        .map(|s| (s.name.clone(), s.query.clone()))
        .collect();
    let current_raw: BTreeMap<_, _> = current
        .searches
        .iter()
        .map(|s| (s.name.clone(), s.query.clone()))
        .collect();

    for op in ops {
        match op {
            DiffOp::Create(s) => {
                println!("{} {}", "+".green(), s.name.green());
                if let Some(raw) = desired_raw.get(&s.name) {
                    println!("{}", raw);
                }
                println!("{}", s.query);
            }
            DiffOp::Delete(s) => {
                println!("{} {}", "-".red(), s.name.red());
                if let Some(raw) = current_raw.get(&s.name) {
                    println!("{}", raw);
                }
                println!("{}", s.query);
            }
            DiffOp::Update { from: _, to } => {
                println!("{} {}", "~".yellow(), to.name.yellow());
                if let Some(raw) = desired_raw.get(&to.name) {
                    println!("{}", raw);
                }
                println!("{}", to.query);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_basic() {
        let mut cur = Config::default();
        cur.vars.insert("tag".into(), "1".into());
        cur.searches.push(Search {
            name: "S1".into(),
            query: "foo {{ tag }}".into(),
        });
        cur.searches.push(Search {
            name: "S2".into(),
            query: "bar".into(),
        });

        let mut des = Config::default();
        des.vars.insert("tag".into(), "2".into());
        des.searches.push(Search {
            name: "S1".into(),
            query: "foo {{ tag }}".into(),
        });
        des.searches.push(Search {
            name: "S3".into(),
            query: "baz".into(),
        });

        let ops = diff_configs(&cur, &des).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(
            matches!(ops[0], DiffOp::Create(_))
                || matches!(ops[1], DiffOp::Create(_))
                || matches!(ops[2], DiffOp::Create(_))
        );
        assert!(ops.iter().any(|o| matches!(o, DiffOp::Update { .. })));
        assert!(ops.iter().any(|o| matches!(o, DiffOp::Delete(_))));
    }
}
