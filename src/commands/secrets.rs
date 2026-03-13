use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;

use crate::{dotfiles, secret};

#[derive(Subcommand)]
pub enum SecretsAction {
    /// List all secret placeholders and their URIs
    List,
    /// Validate all secrets can be fetched from their backends
    Validate,
    /// Add a new secret placeholder
    Add {
        /// Placeholder name (e.g. GITHUB_EMAIL)
        name: String,
        /// Secret URI (e.g. pass://vault/item/field  op://vault/item/field  env://VAR)
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
        SecretsAction::List => list(),
        SecretsAction::Validate => validate(),
        SecretsAction::Add { name, uri } => add(name, uri),
        SecretsAction::Remove { name } => remove(name),
    }
}

fn list() -> Result<()> {
    let secrets = dotfiles::read_secrets()?;

    if secrets.secrets.is_empty() {
        println!(
            "No secrets configured. Run {} to add one.",
            "dotf secrets add <name> <uri>".cyan()
        );
        return Ok(());
    }

    let name_width = secrets
        .secrets
        .keys()
        .map(|k| k.len())
        .max()
        .unwrap_or(12)
        .max(12);
    let uri_width = secrets
        .secrets
        .values()
        .map(|v| v.len())
        .max()
        .unwrap_or(10)
        .max(10);
    let backend_width = "BACKEND".len();

    println!(
        "{:<name_width$}  {:<uri_width$}  {}",
        "PLACEHOLDER".bold(),
        "SECRET URI".bold(),
        "BACKEND".bold()
    );
    println!(
        "{}",
        "─"
            .repeat(name_width + uri_width + backend_width + 4)
            .dimmed()
    );

    let mut entries: Vec<_> = secrets.secrets.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    for (name, uri) in entries {
        println!(
            "{:<name_width$}  {:<uri_width$}  {}",
            name.cyan(),
            uri,
            secret::backend_name(uri).dimmed()
        );
    }

    Ok(())
}

fn validate() -> Result<()> {
    let secrets = dotfiles::read_secrets()?;

    if secrets.secrets.is_empty() {
        println!("No secrets to validate.");
        return Ok(());
    }

    let mut passed = 0usize;
    let mut failed = 0usize;

    let mut entries: Vec<_> = secrets.secrets.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    for (name, uri) in entries {
        match secret::fetch(uri) {
            Ok(_) => {
                println!("{} {} ({})", "✓".green(), name.cyan(), uri.dimmed());
                passed += 1;
            }
            Err(e) => {
                let err_str = e.to_string();
                println!(
                    "{} {} ({}) — {}",
                    "✗".red(),
                    name.cyan(),
                    uri.dimmed(),
                    err_str.red()
                );
                failed += 1;
            }
        }
    }

    println!();
    println!(
        "{} passed, {} failed",
        passed.to_string().green(),
        if failed > 0 {
            failed.to_string().red()
        } else {
            failed.to_string().green()
        }
    );

    if failed > 0 {
        anyhow::bail!("{failed} secret(s) failed validation");
    }

    Ok(())
}

fn add(name: String, uri: String) -> Result<()> {
    let mut secrets = dotfiles::read_secrets()?;
    let existed = secrets.secrets.contains_key(&name);
    secrets.secrets.insert(name.clone(), uri.clone());
    dotfiles::write_secrets(&secrets).context("Failed to write .secrets.toml")?;

    if existed {
        println!("{} Updated {} -> {}", "✓".green(), name.cyan(), uri);
    } else {
        println!("{} Added {} -> {}", "✓".green(), name.cyan(), uri);
    }
    Ok(())
}

fn remove(name: String) -> Result<()> {
    let mut secrets = dotfiles::read_secrets()?;
    if secrets.secrets.remove(&name).is_none() {
        anyhow::bail!("Secret '{}' not found in .secrets.toml", name);
    }
    dotfiles::write_secrets(&secrets).context("Failed to write .secrets.toml")?;
    println!("{} Removed {}", "✓".green(), name.cyan());
    Ok(())
}
