mod cli;
mod cmd_auth;
mod cmd_diff;
mod cmd_pull;
mod cmd_push;
mod cmd_sync;
mod config;
mod config_file;

use anyhow::Result;
use clap::Parser;
use cli::{AuthCommands, Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pull => cmd_pull::run().await,
        Commands::Sync => cmd_sync::run().await,
        Commands::Diff => cmd_diff::run().await,
        Commands::Push => cmd_push::run().await,
        Commands::Auth(auth_cmd) => match auth_cmd {
            AuthCommands::Login => cmd_auth::login().await,
            AuthCommands::Logout => cmd_auth::logout().await,
        },
    }
}
