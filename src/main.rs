use dotf::commands;
use dotf::dotfiles::DotfContext;
use dotf::runner::SystemRunner;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};

#[derive(Parser)]
#[command(name = "dotf", about = "Personal dotfiles manager", version)]
struct Cli {
    /// Use a project-local .dotf/ directory instead of ~/dotfiles.
    /// Pass a directory path (e.g. --dir . for the current directory).
    #[arg(long, global = true, value_name = "PATH")]
    dir: Option<std::path::PathBuf>,

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

fn main() {
    let cli = Cli::parse();

    let ctx = match cli.dir {
        Some(dir) => {
            let abs = match std::path::absolute(&dir) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: --dir path '{}': {}", dir.display(), e);
                    std::process::exit(1);
                }
            };
            DotfContext::local(abs)
        }
        None => DotfContext::global(),
    };

    let result = match cli.command {
        Command::Init => commands::init::run(&SystemRunner, &ctx),
        Command::Config { path } => commands::config::run(&ctx, path),
        Command::Modify { name } => commands::modify::run(&SystemRunner, &ctx, name),
        Command::Diff { name } => commands::diff::run(&ctx, name),
        Command::Remove { name } => commands::remove::run(&ctx, name),
        Command::Sync => commands::sync::run(&SystemRunner, &ctx),
        Command::Secrets { action } => commands::secrets::run(&ctx, action),
        Command::Status => commands::status::run(&ctx),
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "dotf", &mut std::io::stdout());
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        // Print error chain (causes) if present.
        let mut source = e.source();
        while let Some(cause) = source {
            eprintln!("  caused by: {cause}");
            source = cause.source();
        }
        std::process::exit(1);
    }
}
