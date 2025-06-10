use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "feedbinctl", author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Fetch saved searches and tags
    Pull,
    /// Fetch and merge into config file
    Sync,
    /// Display planned changes between config and Feedbin
    Diff,
    /// Apply changes to Feedbin
    Push,
    /// Authentication commands
    #[command(subcommand)]
    Auth(AuthCommands),
}

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// Login and store token in keyring
    Login,
    /// Logout and remove token from keyring
    Logout,
}
