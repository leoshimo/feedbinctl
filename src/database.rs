use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::api::{FeedEntry, Subscription};

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Serialize, PartialEq)]
pub struct EntrySummary {
    pub id: i64,
    pub feed_id: i64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub feed_title: Option<String>,
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
    pub published: String,
    pub created_at: String,
}

pub fn default_path() -> Result<PathBuf> {
    let file_name = if cfg!(debug_assertions) {
        "feedbin-dev.sqlite"
    } else {
        "feedbin.sqlite"
    };
    Ok(path_for_profile(&xdg_data_home()?, file_name))
}

fn xdg_data_home() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME")
        && !value.is_empty()
    {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            bail!("XDG_DATA_HOME must be an absolute path");
        }
        return Ok(path);
    }

    let dirs = BaseDirs::new().context("could not determine the home directory")?;
    Ok(dirs.home_dir().join(".local").join("share"))
}

fn path_for_profile(data_home: &Path, file_name: &str) -> PathBuf {
    data_home.join("feedbinctl").join(file_name)
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        initialize_schema(&connection)?;
        Ok(Self { connection })
    }

    pub fn cursor(&self) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT value FROM metadata WHERE key = 'entries_cursor'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn store_feeds(&mut self, subscriptions: &[Subscription]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for subscription in subscriptions {
            transaction.execute(
                r#"
                INSERT INTO feeds (feed_id, title, feed_url, site_url)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(feed_id) DO UPDATE SET
                    title = excluded.title,
                    feed_url = excluded.feed_url,
                    site_url = excluded.site_url
                "#,
                params![
                    subscription.feed_id,
                    subscription.title,
                    subscription.feed_url,
                    subscription.site_url,
                ],
            )?;
        }
        transaction.execute(
            r#"
            UPDATE entries
            SET feed_title = (SELECT title FROM feeds WHERE feeds.feed_id = entries.feed_id)
            WHERE EXISTS (
                SELECT 1 FROM feeds
                WHERE feeds.feed_id = entries.feed_id
                  AND feeds.title IS NOT entries.feed_title
            )
            "#,
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn store_entries(&mut self, entries: &[FeedEntry]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for entry in entries {
            store_entry(&transaction, entry)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn set_cursor(&self, cursor: &str) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO metadata (key, value) VALUES ('entries_cursor', ?1)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            [cursor],
        )?;
        Ok(())
    }

    pub fn entries(&self, limit: usize, before: Option<i64>) -> Result<Vec<EntrySummary>> {
        if limit == 0 {
            return Ok(vec![]);
        }

        if let Some(before) = before {
            let anchor: Option<(String, i64)> = self
                .connection
                .query_row(
                    "SELECT created_at, id FROM entries WHERE id = ?1",
                    [before],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (created_at, id) = anchor
                .with_context(|| format!("entry {before} is not present in the local index"))?;
            let mut statement = self.connection.prepare(&format!(
                "{} WHERE e.created_at < ?1 OR (e.created_at = ?1 AND e.id < ?2) ORDER BY e.created_at DESC, e.id DESC LIMIT ?3",
                summary_select()
            ))?;
            let mut rows = statement.query(params![created_at, id, limit as i64])?;
            collect_summaries(&mut rows)
        } else {
            let mut statement = self.connection.prepare(&format!(
                "{} ORDER BY e.created_at DESC, e.id DESC LIMIT ?1",
                summary_select()
            ))?;
            let mut rows = statement.query([limit as i64])?;
            collect_summaries(&mut rows)
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<EntrySummary>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        if query.trim().is_empty() {
            bail!("search query must not be empty");
        }

        let mut statement = self.connection.prepare(
            r#"
            SELECT
                e.id, e.feed_id, e.title, e.url, e.author, e.feed_title,
                f.feed_url, f.site_url, e.published, e.created_at
            FROM entries_fts
            JOIN entries e ON e.id = entries_fts.rowid
            LEFT JOIN feeds f ON f.feed_id = e.feed_id
            WHERE entries_fts MATCH ?1
            ORDER BY bm25(entries_fts), e.created_at DESC, e.id DESC
            LIMIT ?2
            "#,
        )?;
        let mut rows = statement.query(params![query, limit as i64])?;
        collect_summaries(&mut rows)
    }

    pub fn checkpoint(&self) -> Result<()> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

fn store_entry(transaction: &Transaction<'_>, entry: &FeedEntry) -> Result<()> {
    transaction.execute(
        r#"
        INSERT INTO entries (
            id, feed_id, title, url, author, feed_title, published, created_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            (SELECT title FROM feeds WHERE feed_id = ?2),
            ?6, ?7
        )
        ON CONFLICT(id) DO UPDATE SET
            feed_id = excluded.feed_id,
            title = excluded.title,
            url = excluded.url,
            author = excluded.author,
            feed_title = excluded.feed_title,
            published = excluded.published,
            created_at = excluded.created_at
        "#,
        params![
            entry.id,
            entry.feed_id,
            entry.title,
            entry.url,
            entry.author,
            entry.published,
            entry.created_at,
        ],
    )?;
    Ok(())
}

fn summary_select() -> &'static str {
    r#"
    SELECT
        e.id, e.feed_id, e.title, e.url, e.author, e.feed_title,
        f.feed_url, f.site_url, e.published, e.created_at
    FROM entries e
    LEFT JOIN feeds f ON f.feed_id = e.feed_id
    "#
}

fn collect_summaries(rows: &mut rusqlite::Rows<'_>) -> Result<Vec<EntrySummary>> {
    let mut entries = Vec::new();
    while let Some(row) = rows.next()? {
        entries.push(EntrySummary {
            id: row.get(0)?,
            feed_id: row.get(1)?,
            title: row.get(2)?,
            url: row.get(3)?,
            author: row.get(4)?,
            feed_title: row.get(5)?,
            feed_url: row.get(6)?,
            site_url: row.get(7)?,
            published: row.get(8)?,
            created_at: row.get(9)?,
        });
    }
    Ok(entries)
}

fn initialize_schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        SCHEMA_VERSION => Ok(()),
        0 if table_exists(connection, "entries")? => migrate_prototype_schema(connection),
        0 => create_schema(connection),
        other => bail!(
            "database schema version {other} is newer than this feedbinctl supports ({SCHEMA_VERSION})"
        ),
    }
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(&format!(
        r#"
        CREATE TABLE feeds (
            feed_id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            feed_url TEXT NOT NULL,
            site_url TEXT NOT NULL
        );

        CREATE TABLE entries (
            id INTEGER PRIMARY KEY,
            feed_id INTEGER NOT NULL,
            title TEXT,
            url TEXT,
            author TEXT,
            feed_title TEXT,
            published TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX entries_created_at ON entries(created_at DESC, id DESC);

        CREATE VIRTUAL TABLE entries_fts USING fts5(
            title, url, author, feed_title,
            content = 'entries', content_rowid = 'id'
        );

        CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
            INSERT INTO entries_fts(rowid, title, url, author, feed_title)
            VALUES (new.id, new.title, new.url, new.author, new.feed_title);
        END;
        CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, title, url, author, feed_title)
            VALUES ('delete', old.id, old.title, old.url, old.author, old.feed_title);
        END;
        CREATE TRIGGER entries_au AFTER UPDATE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, title, url, author, feed_title)
            VALUES ('delete', old.id, old.title, old.url, old.author, old.feed_title);
            INSERT INTO entries_fts(rowid, title, url, author, feed_title)
            VALUES (new.id, new.title, new.url, new.author, new.feed_title);
        END;

        CREATE TABLE metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        PRAGMA user_version = {SCHEMA_VERSION};
        "#
    ))?;
    Ok(())
}

fn migrate_prototype_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        BEGIN IMMEDIATE;
        DROP TRIGGER IF EXISTS entries_ai;
        DROP TRIGGER IF EXISTS entries_ad;
        DROP TRIGGER IF EXISTS entries_au;
        DROP TABLE IF EXISTS entries_fts;
        ALTER TABLE entries RENAME TO entries_prototype;
        ALTER TABLE subscriptions RENAME TO subscriptions_prototype;
        ALTER TABLE metadata RENAME TO metadata_prototype;
        COMMIT;
        "#,
    )?;
    create_schema(connection)?;
    connection.execute_batch(
        r#"
        BEGIN IMMEDIATE;
        INSERT INTO feeds (feed_id, title, feed_url, site_url)
        SELECT feed_id, title, feed_url, site_url FROM subscriptions_prototype;

        INSERT INTO entries (
            id, feed_id, title, url, author, feed_title, published, created_at
        )
        SELECT
            e.id, e.feed_id, e.title, e.url, e.author, s.title,
            e.published, e.created_at
        FROM entries_prototype e
        LEFT JOIN subscriptions_prototype s ON s.feed_id = e.feed_id;

        INSERT INTO metadata (key, value)
        SELECT 'entries_cursor', value
        FROM metadata_prototype
        WHERE key = 'entries_since';

        DROP TABLE entries_prototype;
        DROP TABLE subscriptions_prototype;
        DROP TABLE metadata_prototype;
        COMMIT;
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_development_and_production_databases() {
        let data_home = Path::new("/home/example/.local/share");
        assert_eq!(
            path_for_profile(data_home, "feedbin-dev.sqlite"),
            data_home.join("feedbinctl/feedbin-dev.sqlite")
        );
        assert_eq!(
            path_for_profile(data_home, "feedbin.sqlite"),
            data_home.join("feedbinctl/feedbin.sqlite")
        );
    }

    fn subscription() -> Subscription {
        Subscription {
            id: 1,
            feed_id: 2,
            title: "Example Feed".to_string(),
            feed_url: "https://example.com/feed".to_string(),
            site_url: "https://example.com".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn entry(id: i64, created_at: &str, title: &str) -> FeedEntry {
        FeedEntry {
            id,
            feed_id: 2,
            title: Some(title.to_string()),
            author: Some("Example Author".to_string()),
            summary: Some("not stored".to_string()),
            content: Some("large HTML is not stored".to_string()),
            url: (id != 2).then(|| format!("https://example.com/{id}")),
            extracted_content_url: None,
            published: created_at.to_string(),
            created_at: created_at.to_string(),
        }
    }

    fn in_memory_database() -> Database {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        Database { connection }
    }

    #[test]
    fn stores_compact_entries_searches_feed_names_and_pages_stably() {
        let mut database = in_memory_database();
        database.store_feeds(&[subscription()]).unwrap();
        database
            .store_entries(&[
                entry(1, "2026-01-03T00:00:00Z", "Distributed soup"),
                entry(2, "2026-01-02T00:00:00Z", "Title without URL"),
                entry(3, "2026-01-01T00:00:00Z", "Oldest"),
            ])
            .unwrap();

        let newest = database.entries(2, None).unwrap();
        assert_eq!(
            newest.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(newest[1].url, None);
        let older = database.entries(2, Some(2)).unwrap();
        assert_eq!(older.iter().map(|entry| entry.id).collect::<Vec<_>>(), [3]);

        let matches = database.search("distributed", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].feed_title.as_deref(), Some("Example Feed"));
        let feed_matches = database.search("feed_title:Example", 10).unwrap();
        assert_eq!(feed_matches.len(), 3);

        database.set_cursor("2026-01-03T00:00:00Z").unwrap();
        assert_eq!(
            database.cursor().unwrap().as_deref(),
            Some("2026-01-03T00:00:00Z")
        );
    }

    #[test]
    fn before_uses_id_to_disambiguate_equal_timestamps() {
        let mut database = in_memory_database();
        database.store_feeds(&[subscription()]).unwrap();
        database
            .store_entries(&[
                entry(2, "2026-01-01T00:00:00Z", "Second"),
                entry(1, "2026-01-01T00:00:00Z", "First"),
            ])
            .unwrap();

        assert_eq!(database.entries(1, None).unwrap()[0].id, 2);
        assert_eq!(database.entries(1, Some(2)).unwrap()[0].id, 1);
    }
}
