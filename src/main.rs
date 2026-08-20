mod api;
mod cli;
mod cmd_auth;
mod commands;
mod database;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth(args) if args.logout => cmd_auth::logout().await,
        Commands::Auth(_) => cmd_auth::login().await,
        Commands::Index(args) => commands::index(args).await,
        Commands::Collections(args) => commands::collections(args),
        Commands::Entries(args) => commands::entries(args),
        Commands::Search(args) => commands::search(args),
        Commands::Save(args) => commands::save(args).await,
        Commands::Entry(args) => commands::entry(args).await,
    }
}
