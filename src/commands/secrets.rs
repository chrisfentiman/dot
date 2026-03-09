use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;

#[derive(Subcommand)]
pub enum SecretsAction {
    /// List all secret placeholders and their pass:// paths
    List,
    /// Validate all secrets exist in Proton Pass
    Validate,
    /// Add a new secret placeholder
    Add {
        /// Placeholder name (e.g. GITHUB_EMAIL)
        name: String,
        /// Proton Pass URI (e.g. pass://personal/github/email)
        uri: String,
    },
    /// Remove a secret placeholder
    Remove {
        /// Placeholder name to remove
        name: String,
    },
}

pub fn run(action: SecretsAction) -> Result<()> {
    match action {
        SecretsAction::List => println!("{}", "dot secrets list — not yet implemented".yellow()),
        SecretsAction::Validate => println!("{}", "dot secrets validate — not yet implemented".yellow()),
        SecretsAction::Add { name, uri } => println!("{}", format!("dot secrets add {name} {uri} — not yet implemented").yellow()),
        SecretsAction::Remove { name } => println!("{}", format!("dot secrets remove {name} — not yet implemented").yellow()),
    }
    Ok(())
}
