mod commands;
mod dotfiles;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dot", about = "Personal dotfiles manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// First-time setup on a new machine
    Init,
    /// Add a config file to be managed
    Config {
        /// Path to the config file to manage (e.g. ~/.gitconfig)
        path: Option<String>,
    },
    /// Open a managed config in $EDITOR
    Modify {
        /// Name of the config to edit
        name: Option<String>,
    },
    /// Pull latest, re-render templates, push local changes
    Sync,
    /// Manage and validate secrets schema
    Secrets {
        #[command(subcommand)]
        action: commands::secrets::SecretsAction,
    },
    /// Show status of managed configs
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => commands::init::run(),
        Command::Config { path } => commands::config::run(path),
        Command::Modify { name } => commands::modify::run(name),
        Command::Sync => commands::sync::run(),
        Command::Secrets { action } => commands::secrets::run(action),
        Command::Status => commands::status::run(),
    }
}
