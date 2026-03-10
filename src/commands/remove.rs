use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use std::fs;

use crate::dotfiles;

pub fn run(name: Option<String>) -> Result<()> {
    let mut symlinks = dotfiles::read_symlinks()?;

    if symlinks.symlinks.is_empty() {
        anyhow::bail!("No managed configs found.");
    }

    let config_name = match name {
        Some(n) => {
            if !symlinks.symlinks.contains_key(&n) {
                anyhow::bail!("Config '{}' is not managed by dotf", n);
            }
            n
        }
        None => {
            let mut names: Vec<String> = symlinks.symlinks.keys().cloned().collect();
            names.sort();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select config to remove")
                .items(&names)
                .default(0)
                .interact()
                .context("Failed to read selection")?;
            names[selection].clone()
        }
    };

    let target_str = symlinks.symlinks[&config_name].clone();
    let configs_dir = dotfiles::configs_dir()?;
    let template_path = configs_dir.join(format!("{config_name}.tmpl"));
    let rendered_path = configs_dir.join(&config_name);
    let link_path = dotfiles::expand_tilde(&target_str)?;

    println!();
    println!("This will:");
    println!("  {} Remove symlink {}", "·".dimmed(), link_path.display());
    println!(
        "  {} Remove template {}",
        "·".dimmed(),
        template_path.display()
    );
    println!(
        "  {} Remove rendered {}",
        "·".dimmed(),
        rendered_path.display()
    );
    println!("  {} Remove from .symlinks.toml", "·".dimmed());
    println!();

    let restore = if rendered_path.exists() {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Restore rendered file to {}?", link_path.display()))
            .default(true)
            .interact()
            .context("Failed to read confirmation")?
    } else {
        false
    };

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Remove '{}' from dotf management?", config_name))
        .default(false)
        .interact()
        .context("Failed to read confirmation")?;

    if !confirmed {
        println!("{} Aborted", "·".dimmed());
        return Ok(());
    }

    // Remove symlink
    if link_path.symlink_metadata().is_ok() {
        fs::remove_file(&link_path)
            .with_context(|| format!("Failed to remove symlink {}", link_path.display()))?;
        println!("{} Removed symlink {}", "✓".green(), link_path.display());
    }

    // Optionally restore the rendered file in place of the symlink
    if restore && rendered_path.exists() {
        fs::copy(&rendered_path, &link_path)
            .with_context(|| format!("Failed to restore file to {}", link_path.display()))?;
        println!("{} Restored file to {}", "✓".green(), link_path.display());
    }

    // Remove template
    if template_path.exists() {
        fs::remove_file(&template_path)
            .with_context(|| format!("Failed to remove {}", template_path.display()))?;
        println!(
            "{} Removed template {}",
            "✓".green(),
            template_path.display()
        );
    }

    // Remove rendered output
    if rendered_path.exists() {
        fs::remove_file(&rendered_path)
            .with_context(|| format!("Failed to remove {}", rendered_path.display()))?;
        println!(
            "{} Removed rendered file {}",
            "✓".green(),
            rendered_path.display()
        );
    }

    // Remove from .symlinks.toml
    symlinks.symlinks.remove(&config_name);
    dotfiles::write_symlinks(&symlinks).context("Failed to update .symlinks.toml")?;
    println!("{} Removed from .symlinks.toml", "✓".green());

    println!();
    println!(
        "{} '{}' is no longer managed by dotf",
        "✓".green().bold(),
        config_name.cyan()
    );
    Ok(())
}
