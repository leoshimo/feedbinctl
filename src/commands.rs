use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::api::{FeedbinClient, SavedSearch};
use crate::cli::{CollectionsArgs, EntriesArgs, EntryArgs, IndexArgs, SaveArgs, SearchArgs};
use crate::database::{Database, default_path};

pub async fn index(args: IndexArgs) -> Result<()> {
    let destination = args.database.unwrap_or(default_path()?);
    let working_path = if args.rebuild {
        prepare_rebuild_path(&destination)?
    } else {
        destination.clone()
    };

    let mut database = Database::open(&working_path)?;
    let since = if args.rebuild {
        None
    } else {
        database.cursor()?
    };
    let client = FeedbinClient::from_stored_credentials()?;

    let subscriptions = client.subscriptions().await?;
    database.store_feeds(&subscriptions)?;
    let saved_searches = client.saved_searches().await?;

    let mut next = None;
    let mut fetched = 0usize;
    let mut entry_ids = HashSet::new();
    let mut newest_cursor = None::<String>;
    let mut reported_total = false;
    let mut expected_total = None;

    loop {
        let page = client
            .entries_page(next.as_deref(), since.as_deref())
            .await?;
        if !reported_total {
            expected_total = page.total;
            match page.total {
                Some(total) => eprintln!("Indexing {total} entries..."),
                None => eprintln!("Indexing entries..."),
            }
            reported_total = true;
        }

        for entry in &page.entries {
            entry_ids.insert(entry.id);
            if newest_cursor
                .as_ref()
                .is_none_or(|cursor| entry.created_at > *cursor)
            {
                newest_cursor = Some(entry.created_at.clone());
            }
        }
        database.store_entries(&page.entries)?;
        fetched += page.entries.len();
        next = page.next;

        if next.is_none() {
            break;
        }
        if fetched.is_multiple_of(1_000) {
            eprintln!("Indexed {fetched} entries...");
        }
    }

    validate_entry_count(expected_total, entry_ids.len(), fetched)?;

    let mut indexed_saved_searches = Vec::with_capacity(saved_searches.len());
    for search in saved_searches {
        let entry_ids = saved_search_entry_ids(&client, &search).await?;
        indexed_saved_searches.push((search, entry_ids));
    }
    database.store_saved_searches(&indexed_saved_searches)?;

    if let Some(cursor) = newest_cursor {
        database.set_cursor(&cursor)?;
    }
    database.checkpoint()?;
    drop(database);

    if args.rebuild {
        replace_database(&working_path, &destination)?;
    }

    println!(
        "Indexed {} entries, {} feeds, and {} saved searches into {}",
        entry_ids.len(),
        subscriptions.len(),
        indexed_saved_searches.len(),
        destination.display()
    );
    Ok(())
}

async fn saved_search_entry_ids(client: &FeedbinClient, search: &SavedSearch) -> Result<Vec<i64>> {
    let mut next = None;
    let mut entry_ids = Vec::new();
    let mut unique = HashSet::new();
    let mut expected_total = None;

    loop {
        let page = client
            .saved_search_entry_ids_page(search.id, next.as_deref())
            .await?;
        expected_total = expected_total.or(page.total);
        for entry_id in page.entry_ids {
            if unique.insert(entry_id) {
                entry_ids.push(entry_id);
            }
        }
        next = page.next;
        if next.is_none() {
            break;
        }
    }

    if let Some(expected) = expected_total
        && unique.len() != expected
    {
        bail!(
            "Feedbin reported {expected} entries for saved search {:?}, but pagination returned {} unique entries",
            search.name,
            unique.len()
        );
    }
    Ok(entry_ids)
}

fn validate_entry_count(expected: Option<usize>, unique: usize, fetched: usize) -> Result<()> {
    if let Some(expected) = expected
        && unique != expected
    {
        bail!(
            "Feedbin reported {expected} entries, but pagination returned {unique} unique entries ({fetched} rows including duplicates); the index cursor was not advanced"
        );
    }
    Ok(())
}

pub fn entries(args: EntriesArgs) -> Result<()> {
    let path = args.database.unwrap_or(default_path()?);
    let entries = Database::open(&path)?.entries(args.limit, args.before, &args.collections)?;
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}

pub fn collections(args: CollectionsArgs) -> Result<()> {
    let path = args.database.unwrap_or(default_path()?);
    let collections = Database::open(&path)?.collections()?;
    println!("{}", serde_json::to_string_pretty(&collections)?);
    Ok(())
}

pub fn search(args: SearchArgs) -> Result<()> {
    let path = args.database.unwrap_or(default_path()?);
    let results = Database::open(&path)?.search(&args.query, args.limit, &args.collections)?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub async fn save(args: SaveArgs) -> Result<()> {
    let entry = FeedbinClient::from_stored_credentials()?
        .save_page(&args.url, args.title.as_deref())
        .await?;
    let path = args.database.unwrap_or(default_path()?);
    Database::open(&path)?.store_entries(std::slice::from_ref(&entry))?;
    println!("{}", serde_json::to_string_pretty(&entry)?);
    Ok(())
}

pub async fn entry(args: EntryArgs) -> Result<()> {
    let entry = FeedbinClient::from_stored_credentials()?
        .entry(args.id)
        .await?;
    println!("{}", serde_json::to_string_pretty(&entry)?);
    Ok(())
}

fn prepare_rebuild_path(destination: &Path) -> Result<PathBuf> {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .context("database path must end in a UTF-8 filename")?;
    let path = destination.with_file_name(format!(".{file_name}.rebuild"));
    remove_if_present(&path)?;
    remove_if_present(&sidecar(&path, "wal"))?;
    remove_if_present(&sidecar(&path, "shm"))?;
    Ok(path)
}

fn replace_database(source: &Path, destination: &Path) -> Result<()> {
    if !destination.exists() {
        std::fs::rename(source, destination).with_context(|| {
            format!(
                "failed to move rebuilt database to {}",
                destination.display()
            )
        })?;
        return Ok(());
    }

    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .context("database path must end in a UTF-8 filename")?;
    let backup = destination.with_file_name(format!(".{file_name}.backup"));
    remove_if_present(&backup)?;
    std::fs::rename(destination, &backup).with_context(|| {
        format!(
            "failed to move existing database {} out of the way",
            destination.display()
        )
    })?;

    if let Err(error) = std::fs::rename(source, destination) {
        let restore = std::fs::rename(&backup, destination);
        if let Err(restore_error) = restore {
            bail!(
                "failed to install rebuilt database ({error}) and restore {} ({restore_error}); backup remains at {}",
                destination.display(),
                backup.display()
            );
        }
        return Err(error).with_context(|| {
            format!(
                "failed to install rebuilt database at {}",
                destination.display()
            )
        });
    }

    remove_if_present(&backup)?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!("-{suffix}"));
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_uses_sibling_files() {
        let destination = Path::new("/tmp/example/feedbin.sqlite");
        assert_eq!(
            prepare_rebuild_path(destination).unwrap(),
            Path::new("/tmp/example/.feedbin.sqlite.rebuild")
        );
    }

    #[test]
    fn pagination_allows_duplicates_when_the_unique_count_matches() {
        validate_entry_count(Some(50_908), 50_908, 50_912).unwrap();
    }

    #[test]
    fn pagination_rejects_an_incomplete_unique_set() {
        let error = validate_entry_count(Some(100), 99, 100).unwrap_err();
        assert!(error.to_string().contains("99 unique entries"));
    }
}
