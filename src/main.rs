use dotf::commands;
use dotf::runner::SystemRunner;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};

#[derive(Parser)]
#[command(name = "dotf", about = "Personal dotfiles manager", version)]
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
    /// Show diff between current rendered configs and fresh template render
    Diff {
        /// Name of the config to diff (omit for interactive selection)
        name: Option<String>,
    },
    /// Remove a config from dotf management
    Remove {
        /// Name of the config to remove (omit for interactive selection)
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
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => commands::init::run(&SystemRunner),
        Command::Config { path } => commands::config::run(path),
        Command::Modify { name } => commands::modify::run(&SystemRunner, name),
        Command::Diff { name } => commands::diff::run(name),
        Command::Remove { name } => commands::remove::run(name),
        Command::Sync => commands::sync::run(&SystemRunner),
        Command::Secrets { action } => commands::secrets::run(action),
        Command::Status => commands::status::run(),
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "dotf", &mut std::io::stdout());
            Ok(())
        }
    }
}
