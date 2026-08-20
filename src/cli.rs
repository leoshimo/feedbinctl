use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::database::CollectionSelector;

const TOP_LEVEL_EXAMPLES: &str = r#"Examples:
  feedbinctl auth
  feedbinctl index
  feedbinctl collections
  feedbinctl entries --limit 50
  feedbinctl entries --collection feed:42
  feedbinctl search 'distributed systems'
  feedbinctl search emacs --collection saved-search:7
  feedbinctl pages add https://example.com/article
  feedbinctl pages list
  feedbinctl pages remove 5154510253
  feedbinctl entry 5154510253

Run `feedbinctl <command> --help` for command-specific examples."#;

#[derive(Parser, Debug)]
#[command(
    name = "feedbinctl",
    author,
    version,
    about = "Index and search your Feedbin entries",
    long_about = "Index compact metadata and collections from Feedbin into a local SQLite database, browse or search that database without a network connection, and fetch full entry content on demand.",
    after_help = TOP_LEVEL_EXAMPLES,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate and store Feedbin credentials in the operating-system keyring
    #[command(
        long_about = "Prompt for Feedbin credentials, validate them against Feedbin, and store them in the operating-system keyring. Use --logout to remove the stored credential. Network commands also accept FEEDBIN_TOKEN=username:password, which takes precedence over the keyring.",
        after_help = "Examples:\n  feedbinctl auth\n  feedbinctl auth --logout"
    )]
    Auth(AuthArgs),

    /// Incrementally update the local SQLite index
    #[command(
        long_about = "Fetch Feedbin subscriptions, saved searches, saved-search membership, and entries, then update the local SQLite index. The first run fetches the full entry history. Later runs request only entries newer than the last completed run, making this the command to schedule from cron. Pages are written as they arrive, but the cursor advances only after the complete run succeeds. Article bodies are not stored; use `entry ID` to fetch one on demand.\n\nRelease builds use $XDG_DATA_HOME/feedbinctl/feedbin.sqlite and debug builds use feedbin-dev.sqlite in the same directory. XDG_DATA_HOME defaults to ~/.local/share. Use --database to override the complete path.",
        after_help = "Examples:\n  feedbinctl index\n  feedbinctl index --rebuild\n  feedbinctl index --database ./feedbin.sqlite"
    )]
    Index(IndexArgs),

    /// List indexed feeds and saved searches
    #[command(
        long_about = "List the Feedbin collections captured by the latest index run without contacting Feedbin. Each JSON object has a kind (`feed` or `saved_search`), an ID accepted by --collection, and its display name. Feed objects include feed_url and site_url; saved searches include their Feedbin query. Feedbin's `Pages` read-later collection appears as a feed because Pages entries have a feed_id.",
        after_help = "Examples:\n  feedbinctl collections\n  feedbinctl entries --collection feed:42\n  feedbinctl search emacs --collection saved-search:7"
    )]
    Collections(CollectionsArgs),

    /// List entries from the local index, newest first
    #[command(
        long_about = "List compact entry metadata from SQLite without contacting Feedbin. The JSON array contains id, feed_id, title, url, author, feed_title, feed_url, site_url, published, and created_at. Use the ID of the final result with --before to retrieve the next stable page. Repeat --collection to include entries from several feeds or saved searches; omit it to use all entries.",
        after_help = "Examples:\n  feedbinctl entries\n  feedbinctl entries --limit 100\n  feedbinctl entries --limit 50 --before 5154510253\n  feedbinctl entries --collection feed:42\n  feedbinctl entries --collection feed:42 --collection saved-search:7"
    )]
    Entries(EntriesArgs),

    /// Full-text search the local index
    #[command(
        long_about = "Search the local SQLite FTS5 index without contacting Feedbin. Searchable columns are title, url, author, and feed_title. Results use the same compact JSON shape as `entries`; pass a returned ID to `entry ID` when full article content is needed. Repeat --collection to search within several feeds or saved searches; omit it to search all entries.\n\nQuery syntax:\n  words separated by spaces use implicit AND\n  \"quoted words\" match a phrase\n  AND, OR, and NOT combine expressions\n  prefix* matches tokens beginning with a prefix\n  NEAR(first second, 5) matches nearby terms\n  column:term restricts a term to one searchable column\n\nColumn filters such as feed_title: are token-based full-text matches, not exact equality filters. Quote the entire shell argument when it contains operators or FTS punctuation.",
        after_help = "Examples:\n  feedbinctl search distributed\n  feedbinctl search 'distributed systems'\n  feedbinctl search '\"actor model\"'\n  feedbinctl search 'rust OR erlang'\n  feedbinctl search 'rust NOT javascript'\n  feedbinctl search 'distrib*'\n  feedbinctl search 'NEAR(actor process, 5)'\n  feedbinctl search 'title:erlang'\n  feedbinctl search 'feed_title:\"Daring Fireball\" apple'\n  feedbinctl search emacs --collection feed:42\n  feedbinctl search emacs --collection saved-search:7"
    )]
    Search(SearchArgs),

    /// Add, remove, or list Feedbin Pages
    #[command(
        subcommand,
        long_about = "Manage Feedbin's remote Pages collection. These commands require network access. `pages add` and `pages remove` also update the local SQLite index after the Feedbin operation succeeds; `pages list` always reads the authoritative collection from Feedbin."
    )]
    Pages(PagesCommand),

    /// Fetch one complete entry directly from Feedbin
    #[command(
        long_about = "Fetch the current complete Feedbin entry by ID, including summary and article content, and print it as JSON. This command requires network access.",
        after_help = "Example:\n  feedbinctl entry 5154510253"
    )]
    Entry(EntryArgs),
}

#[derive(Args, Debug)]
pub struct AuthArgs {
    /// Remove the stored credential instead of prompting for one
    #[arg(long)]
    pub logout: bool,
}

#[derive(Args, Debug)]
pub struct IndexArgs {
    /// Build a complete replacement database instead of using the saved cursor
    #[arg(long)]
    pub rebuild: bool,

    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct CollectionsArgs {
    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct EntriesArgs {
    /// Return entries older than this indexed entry ID
    #[arg(long, value_name = "ENTRY_ID")]
    pub before: Option<i64>,

    /// Maximum number of entries to return
    #[arg(long, default_value_t = 50, value_name = "COUNT")]
    pub limit: usize,

    /// Restrict results to a feed or saved search; may be repeated
    #[arg(long = "collection", value_name = "KIND:ID")]
    pub collections: Vec<CollectionSelector>,

    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// SQLite FTS5 query
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Maximum number of matches to return
    #[arg(long, default_value_t = 20, value_name = "COUNT")]
    pub limit: usize,

    /// Restrict results to a feed or saved search; may be repeated
    #[arg(long = "collection", value_name = "KIND:ID")]
    pub collections: Vec<CollectionSelector>,

    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum PagesCommand {
    /// Add a URL to Feedbin Pages
    #[command(
        long_about = "Create a Page through the Feedbin API, print the resulting entry as JSON, and upsert its compact metadata into the local SQLite index. Twitter/X Pages with an unhelpful Feedbin title are given a deterministic local title derived from the saved webpage itself.",
        after_help = "Example:\n  feedbinctl pages add https://example.com/article"
    )]
    Add(PageAddArgs),

    /// Remove an entry from Feedbin Pages
    #[command(
        long_about = "Delete a Page through the Feedbin API, then remove the same entry ID from the local SQLite index if that index exists. The entry ID is returned by `pages add` and `pages list`.",
        after_help = "Example:\n  feedbinctl pages remove 5154510253"
    )]
    Remove(PageRemoveArgs),

    /// List the most recent Feedbin Pages
    #[command(
        long_about = "Fetch entries from the authoritative remote Feedbin Pages collection and print compact JSON metadata. Results are newest first. Use the ID of the final result with --before to retrieve the next page. This command does not read or update SQLite; use `entries --collection feed:ID` for the offline indexed view.",
        after_help = "Examples:\n  feedbinctl pages list\n  feedbinctl pages list --limit 100\n  feedbinctl pages list --limit 50 --before 5154510253"
    )]
    List(PageListArgs),
}

#[derive(Args, Debug)]
pub struct PageAddArgs {
    /// URL to save to Feedbin Pages
    pub url: String,

    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PageRemoveArgs {
    /// Feedbin Page entry ID
    pub id: i64,

    /// Override the build-specific XDG database path
    #[arg(long, value_name = "PATH")]
    pub database: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PageListArgs {
    /// Maximum number of Pages to return
    #[arg(long, default_value_t = 50, value_name = "COUNT")]
    pub limit: usize,

    /// Return Pages older than this entry ID
    #[arg(long, value_name = "ENTRY_ID")]
    pub before: Option<i64>,
}

#[derive(Args, Debug)]
pub struct EntryArgs {
    /// Feedbin entry ID, as returned by `entries` or `search`
    pub id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn parses_stable_pagination_arguments() {
        let cli = Cli::try_parse_from(["feedbinctl", "entries", "--limit", "25", "--before", "42"])
            .unwrap();

        let Commands::Entries(args) = cli.command else {
            panic!("expected entries command");
        };
        assert_eq!(args.limit, 25);
        assert_eq!(args.before, Some(42));
    }

    #[test]
    fn parses_nested_pages_commands_without_a_title_option() {
        let cli =
            Cli::try_parse_from(["feedbinctl", "pages", "add", "https://example.com/article"])
                .unwrap();
        let Commands::Pages(PagesCommand::Add(args)) = cli.command else {
            panic!("expected pages add command");
        };
        assert_eq!(args.url, "https://example.com/article");

        assert!(
            Cli::try_parse_from([
                "feedbinctl",
                "pages",
                "add",
                "https://example.com/article",
                "--title",
                "Misleading title",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["feedbinctl", "save", "https://example.com/article"]).is_err()
        );

        let cli = Cli::try_parse_from([
            "feedbinctl",
            "pages",
            "list",
            "--limit",
            "25",
            "--before",
            "42",
        ])
        .unwrap();
        let Commands::Pages(PagesCommand::List(args)) = cli.command else {
            panic!("expected pages list command");
        };
        assert_eq!(args.limit, 25);
        assert_eq!(args.before, Some(42));
    }

    #[test]
    fn long_help_describes_agent_discoverable_workflow() {
        let help = Cli::command().render_long_help().to_string();
        for command in [
            "auth",
            "index",
            "collections",
            "entries",
            "search",
            "pages",
            "entry",
        ] {
            assert!(help.contains(command), "missing {command} from help");
        }
        assert!(help.contains("feedbinctl <command> --help"));
    }

    #[test]
    fn command_help_explains_machine_workflows() {
        let mut command = Cli::command();

        let index = command
            .find_subcommand_mut("index")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(index.contains("cron"));
        assert!(index.contains("entry ID"));
        assert!(index.contains("feedbinctl/feedbin.sqlite"));
        assert!(index.contains("feedbin-dev.sqlite"));

        let entries = command
            .find_subcommand_mut("entries")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(entries.contains("feed_url"));
        assert!(entries.contains("--before"));
        assert!(entries.contains("--collection"));

        let collections = command
            .find_subcommand_mut("collections")
            .unwrap()
            .render_long_help()
            .to_string();

        let pages = command
            .find_subcommand_mut("pages")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(pages.contains("remote Pages"));
        assert!(pages.contains("pages list"));
        assert!(collections.contains("Pages"));

        let search = command
            .find_subcommand_mut("search")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(search.contains("compact JSON"));
        assert!(search.contains("entry ID"));
        assert!(search.contains("--collection"));
        for syntax in [
            "title, url, author, and feed_title",
            "implicit AND",
            "quoted words",
            "AND, OR, and NOT",
            "prefix*",
            "NEAR(first second, 5)",
            "column:term",
            "not exact equality",
        ] {
            assert!(search.contains(syntax), "missing {syntax} from search help");
        }

        let pages_add = command
            .find_subcommand_mut("pages")
            .unwrap()
            .find_subcommand_mut("add")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(pages_add.contains("Feedbin API"));
        assert!(pages_add.contains("local SQLite index"));
        assert!(!pages_add.contains("--title"));
    }

    #[test]
    fn parses_repeatable_collection_filters() {
        let cli = Cli::try_parse_from([
            "feedbinctl",
            "entries",
            "--collection",
            "feed:42",
            "--collection",
            "saved-search:7",
        ])
        .unwrap();

        let Commands::Entries(args) = cli.command else {
            panic!("expected entries command");
        };
        assert_eq!(
            args.collections,
            [
                CollectionSelector::Feed(42),
                CollectionSelector::SavedSearch(7)
            ]
        );
    }
}
