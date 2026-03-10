use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Select, theme::ColorfulTheme};
use std::process::Command;

use crate::dotfiles;

pub fn run(name: Option<String>) -> Result<()> {
    let symlinks = dotfiles::read_symlinks()?;

    let config_name = match name {
        Some(n) => n,
        None => {
            let mut names: Vec<String> = symlinks.symlinks.keys().cloned().collect();
            names.sort();
            if names.is_empty() {
                anyhow::bail!("No managed configs found. Run `dot config <path>` to add one.");
            }
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select config to modify")
                .items(&names)
                .default(0)
                .interact()
                .context("Failed to read selection")?;
            names[selection].clone()
        }
    };

    let configs_dir = dotfiles::configs_dir()?;
    let template_path = configs_dir.join(format!("{config_name}.tmpl"));

    if !template_path.exists() {
        anyhow::bail!("Template not found: {}", template_path.display());
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(&editor)
        .arg(&template_path)
        .status()
        .with_context(|| format!("Failed to open editor: {editor}"))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    let secrets = dotfiles::read_secrets()?;
    let output_path = configs_dir.join(&config_name);
    dotfiles::render_and_write(&template_path, &output_path, &secrets)
        .with_context(|| format!("Failed to re-render {config_name}"))?;
    println!("{} Re-rendered {}", "✓".green(), config_name);

    if let Some(target_str) = symlinks.symlinks.get(&config_name) {
        let link_path = dotfiles::expand_tilde(target_str)?;
        dotfiles::ensure_symlink(&output_path, &link_path)
            .with_context(|| format!("Failed to update symlink for {config_name}"))?;
        println!("{} Symlink up to date: {} -> {}", "✓".green(), link_path.display(), output_path.display());
    }

    println!("{} {} updated", "✓".green().bold(), config_name.cyan());
    Ok(())
}
