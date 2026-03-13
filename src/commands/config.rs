use crate::dotfiles;
use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{Confirm, Input, Password, theme::ColorfulTheme};
use std::fs;

pub fn run(path: Option<String>) -> Result<()> {
    let raw_path = match path {
        Some(p) => p,
        None => Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Path to config file")
            .interact_text()
            .context("Failed to read path")?,
    };

    let source_path = dotfiles::expand_tilde(&raw_path)?;
    if !source_path.exists() {
        anyhow::bail!("File not found: {}", source_path.display());
    }

    let filename = source_path
        .file_name()
        .ok_or_else(|| anyhow!("Cannot determine filename for {}", source_path.display()))?
        .to_string_lossy()
        .to_string();

    let mut content = fs::read_to_string(&source_path)
        .with_context(|| format!("Failed to read {}", source_path.display()))?;

    println!();
    println!("File contents of {}:", source_path.display());
    println!("{}", "─".repeat(60).dimmed());
    println!("{}", content);
    println!("{}", "─".repeat(60).dimmed());
    println!();

    let mut new_secrets: Vec<(String, String)> = Vec::new();

    loop {
        let has_secret = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Does this file contain a secret value you want to inject?")
            .default(false)
            .interact()
            .context("Failed to read confirmation")?;

        if !has_secret {
            break;
        }

        let secret_value: String = Password::with_theme(&ColorfulTheme::default())
            .with_prompt(
                "Paste the plaintext value to replace in the file (not stored — used to identify it)",
            )
            .interact()
            .context("Failed to read secret value")?;

        if !content.contains(&secret_value) {
            println!("{} Value not found in file, try again", "!".yellow());
            continue;
        }

        let placeholder: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Placeholder name (e.g. GITHUB_EMAIL)")
            .validate_with(|input: &String| -> std::result::Result<(), &str> {
                if dotfiles::is_valid_placeholder_name(input) {
                    Ok(())
                } else {
                    Err("Placeholder name must be non-empty and contain only letters, digits, and underscores")
                }
            })
            .interact_text()
            .context("Failed to read placeholder name")?;

        let uri: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(
                "Secret URI (e.g. pass://vault/item/field  op://vault/item/field  env://VAR)",
            )
            .interact_text()
            .context("Failed to read secret URI")?;

        content = content.replace(&secret_value, &format!("{{{{{placeholder}}}}}"));
        new_secrets.push((placeholder, uri));
    }

    let configs_dir = dotfiles::configs_dir()?;
    fs::create_dir_all(&configs_dir).context("Failed to create configs dir")?;

    let template_path = configs_dir.join(format!("{filename}.tmpl"));
    fs::write(&template_path, &content)
        .with_context(|| format!("Failed to write template {}", template_path.display()))?;
    println!(
        "{} Template written to {}",
        "✓".green(),
        template_path.display()
    );

    let mut secrets = dotfiles::read_secrets()?;
    for (name, uri) in &new_secrets {
        secrets.secrets.insert(name.clone(), uri.clone());
    }
    dotfiles::write_secrets(&secrets)?;
    if !new_secrets.is_empty() {
        println!(
            "{} Added {} secret(s) to .secrets.toml",
            "✓".green(),
            new_secrets.len()
        );
    }

    // Derive the symlink target from the original file's actual location
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let target_str = if let Ok(rel) = source_path.strip_prefix(&home) {
        format!("~/{}", rel.display())
    } else {
        source_path.to_string_lossy().to_string()
    };

    let mut symlinks = dotfiles::read_symlinks()?;
    symlinks
        .symlinks
        .insert(filename.clone(), target_str.clone());
    dotfiles::write_symlinks(&symlinks)?;
    println!("{} Added symlink mapping to .symlinks.toml", "✓".green());

    let output_path = configs_dir.join(&filename);
    dotfiles::render_and_write(&template_path, &output_path, &secrets)
        .with_context(|| format!("Failed to render template for {filename}"))?;
    println!(
        "{} Rendered template to {}",
        "✓".green(),
        output_path.display()
    );

    let link_path = dotfiles::expand_tilde(&target_str)?;
    dotfiles::ensure_symlink(&output_path, &link_path)
        .with_context(|| format!("Failed to create symlink for {filename}"))?;
    println!(
        "{} Symlinked {} -> {}",
        "✓".green(),
        link_path.display(),
        output_path.display()
    );

    println!();
    println!(
        "{} {} is now managed by dot",
        "✓".green().bold(),
        filename.cyan()
    );
    Ok(())
}
