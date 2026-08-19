# Working on feedbinctl

`feedbinctl` deliberately has a small command surface:

- `auth` validates and stores credentials; `auth --logout` removes them.
- `index` is the only bulk network operation. It incrementally writes compact
  metadata to SQLite and is safe to repeat.
- `entries` pages through locally indexed entries with `--before ENTRY_ID`.
- `search` queries the local FTS5 index.
- `entry ID` fetches full content directly from Feedbin.

Run `feedbinctl --help` and `feedbinctl <command> --help` before scripting the
CLI. Commands returning data write JSON to stdout. Index progress is written to
stderr, leaving the final status line on stdout.

## Behavioral invariants

- Never store article `content`, `summary`, or extracted content in SQLite.
- Accept nullable entry URLs; Feedbin contains real entries without one.
- Write index pages as they arrive, but advance `entries_cursor` only after the
  complete paginated request succeeds.
- Preserve Feedbin's exact `created_at` cursor string.
- Local pagination is ordered by `(created_at DESC, id DESC)` and anchored by
  the entry supplied to `--before`.
- `entries` and `search` must not require credentials or network access.
- A rebuild must not replace the existing database until the new index is
  complete.

## Feedbin references

- API: <https://github.com/feedbin/feedbin-api>
- Entries: <https://github.com/feedbin/feedbin-api/blob/master/content/entries.md>
- Subscriptions: <https://github.com/feedbin/feedbin-api/blob/master/content/subscriptions.md>

## Verification

Run all of the following and fix every warning or failure:

```sh
cargo fmt --all --check
cargo check
cargo build
cargo lint
cargo test
```
