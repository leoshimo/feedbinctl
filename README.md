# feedbinctl

Index compact metadata from Feedbin into SQLite, search it locally, and fetch
complete entries on demand.

*Disclaimer:* This project is an exercise with Codex.

## Install

```sh
cargo install --path .
```

For development, replace `feedbinctl` in the examples below with `cargo run --`.
Both builds use the same operating-system keyring entry.

## Authenticate

```sh
feedbinctl auth
feedbinctl auth --logout
```

`auth` validates your Feedbin username and password before storing them in the
operating-system keyring. For non-interactive environments, `FEEDBIN_TOKEN`
may instead contain `username:password`.

## Maintain the index

```sh
feedbinctl index
```

The first run downloads the complete entry history. Later runs use the exact
`created_at` timestamp from the last completed run as Feedbin's `since` cursor.
This makes the command suitable for cron:

```cron
*/15 * * * * /usr/local/bin/feedbinctl index
```

Entries are fetched and committed one API page at a time. The cursor advances
only after all pages succeed, so an interrupted run can safely be repeated.
Upserts make repeated pages harmless.

To build a complete replacement database while leaving the current database in
place until the download succeeds:

```sh
feedbinctl index --rebuild
```

The default database uses an XDG-style data directory and is separated by
build profile:

- Installed binaries and `cargo run --release`:
  `${XDG_DATA_HOME:-~/.local/share}/feedbinctl/feedbin.sqlite`
- Debug builds such as `cargo run`:
  `${XDG_DATA_HOME:-~/.local/share}/feedbinctl/feedbin-dev.sqlite`

This keeps development indexing separate from the index used by an installed
binary. Both profiles continue to use the same operating-system keyring entry.
`XDG_DATA_HOME`, when set, must be an absolute path.

Use `--database PATH` on `index`, `entries`, or `search` to override it.

## Browse recent entries

```sh
feedbinctl entries --limit 50
```

`entries` reads SQLite and never contacts Feedbin. Results are ordered by
`created_at` and then entry ID, newest first. For the next stable page, pass the
last result's ID:

```sh
feedbinctl entries --limit 50 --before 5154510253
```

This keyset pagination remains stable when a later `index` run inserts newer
entries.

## Search

```sh
feedbinctl search distributed
feedbinctl search 'distributed AND systems'
feedbinctl search 'feed_title:example' --limit 50
```

`search` is local and uses SQLite FTS5 syntax over title, URL, author, and feed
title. Both `entries` and `search` print JSON arrays containing compact metadata:

```json
{
  "id": 5154510253,
  "feed_id": 42,
  "title": "Distributed systems",
  "url": "https://example.com/post",
  "author": "Example Author",
  "feed_title": "Example Feed",
  "feed_url": "https://example.com/feed.xml",
  "site_url": "https://example.com",
  "published": "2026-09-01T12:00:00Z",
  "created_at": "2026-09-01T12:01:00Z"
}
```

## Fetch full content

Search results intentionally omit article bodies. Fetch the selected entry from
Feedbin by ID:

```sh
feedbinctl entry 5154510253
```

`entry` contacts Feedbin and prints the complete current object, including
`summary`, `content`, and `extracted_content_url`.

Every command documents its behavior and examples through Clap:

```sh
feedbinctl --help
feedbinctl index --help
feedbinctl entries --help
feedbinctl search --help
feedbinctl entry --help
```
