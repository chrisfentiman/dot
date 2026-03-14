use dotf::commands;
use dotf::dotfiles::{self, DotfContext};
use dotf::runner::SystemRunner;
use dotf::ui::UI;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};

#[derive(Parser)]
#[command(name = "dotf", about = "Personal dotfiles manager", version)]
struct Cli {
    /// Force global mode (~/.dotf) regardless of current directory
    #[arg(long, short = 'G', global = true)]
    global: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// First-time setup on a new machine
    Init {
        /// Optional path for project-local init (omit for global ~/.dotf/ init)
        path: Option<std::path::PathBuf>,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Commands that auto-detect scope from the current directory
    #[command(flatten)]
    Scoped(ScopedCommand),
}

#[derive(Subcommand)]
enum ScopedCommand {
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
}

fn unwrap_or_exit<T>(result: anyhow::Result<T>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            let mut source = e.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let ui = UI::new();
    let result = match cli.command {
        Command::Init { path } => {
            let ctx = match path {
                Some(p) => unwrap_or_exit(DotfContext::local_from_path(&p)),
                None => DotfContext::global(),
            };
            commands::init::run(&ui, &SystemRunner, &ctx)
        }
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "dotf", &mut std::io::stdout());
            Ok(())
        }
        Command::Scoped(cmd) => {
            let ctx = if cli.global {
                DotfContext::global()
            } else {
                unwrap_or_exit(dotfiles::resolve_context())
            };
            match cmd {
                ScopedCommand::Config { path } => commands::config::run(&ui, &ctx, path),
                ScopedCommand::Modify { name } => {
                    commands::modify::run(&ui, &SystemRunner, &ctx, name)
                }
                ScopedCommand::Diff { name } => commands::diff::run(&ui, &ctx, name),
                ScopedCommand::Remove { name } => commands::remove::run(&ui, &ctx, name),
                ScopedCommand::Sync => commands::sync::run(&ui, &SystemRunner, &ctx),
                ScopedCommand::Secrets { action } => commands::secrets::run(&ui, &ctx, action),
                ScopedCommand::Status => commands::status::run(&ui, &ctx),
            }
        }
    };

    unwrap_or_exit(result);
}
