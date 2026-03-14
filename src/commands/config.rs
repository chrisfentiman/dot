use crate::dotfiles;
use crate::dotfiles::{DotfContext, DotfMode};
use crate::ui::UI;
use anyhow::{Context, Result, anyhow};
use dialoguer::{Confirm, Input, Password, theme::ColorfulTheme};
use std::fs;

pub fn run(ui: &UI, ctx: &DotfContext, path: Option<String>) -> Result<()> {
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

    ui.action("Reading", source_path.display());

    let mut content = fs::read_to_string(&source_path)
        .with_context(|| format!("Failed to read {}", source_path.display()))?;

    ui.blank();
    ui.raw(ui.dim("─".repeat(60)));
    ui.raw(&content);
    ui.raw(ui.dim("─".repeat(60)));
    ui.blank();

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
            ui.warn("Warning", "Value not found in file, try again");
            continue;
        }

        let match_count = content.matches(&secret_value).count();
        if match_count > 1 {
            ui.warn(
                "Warning",
                format!(
                    "Value appears {} times in the file — all occurrences will be replaced",
                    match_count
                ),
            );
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
            .validate_with(|input: &String| -> std::result::Result<(), &str> {
                if dotfiles::is_valid_secret_uri(input) {
                    Ok(())
                } else {
                    Err("URI must start with pass://, op://, bw://, or env://")
                }
            })
            .interact_text()
            .context("Failed to read secret URI")?;

        content = content.replace(&secret_value, &format!("{{{{{placeholder}}}}}"));
        new_secrets.push((placeholder, uri));
    }

    let configs_dir = ctx.configs_dir()?;
    fs::create_dir_all(&configs_dir).context("Failed to create configs dir")?;

    let template_path = configs_dir.join(format!("{filename}.tmpl"));
    fs::write(&template_path, &content)
        .with_context(|| format!("Failed to write template {}", template_path.display()))?;
    ui.action("Creating", format!("template {}", template_path.display()));

    let mut secrets = ctx.read_secrets()?;
    for (name, uri) in &new_secrets {
        secrets.secrets.insert(name.clone(), uri.clone());
    }
    ctx.write_secrets(&secrets)?;
    if !new_secrets.is_empty() {
        ui.action(
            "Added",
            format!("{} secret(s) to .secrets.toml", new_secrets.len()),
        );
    }

    // Derive the symlink target from the original file's actual location
    let target_str = match &ctx.mode {
        DotfMode::Global => {
            let home = dirs::home_dir().context("Cannot determine home directory")?;
            if let Ok(rel) = source_path.strip_prefix(&home) {
                format!("~/{}", rel.display())
            } else {
                source_path.to_string_lossy().to_string()
            }
        }
        DotfMode::Local(root) => {
            let root_canon = root.canonicalize().unwrap_or_else(|_| root.clone());
            let source_canon = source_path.canonicalize().unwrap_or(source_path.clone());
            if let Ok(rel) = source_canon.strip_prefix(&root_canon) {
                rel.display().to_string()
            } else {
                anyhow::bail!(
                    "File {} is outside the project root {}",
                    source_path.display(),
                    root.display()
                );
            }
        }
    };

    let mut symlinks = ctx.read_symlinks()?;
    symlinks
        .symlinks
        .insert(filename.clone(), target_str.clone());
    ctx.write_symlinks(&symlinks)?;
    ui.action("Added", "symlink mapping to .symlinks.toml");

    let output_path = configs_dir.join(&filename);
    dotfiles::render_and_write(&template_path, &output_path, &secrets)
        .with_context(|| format!("Failed to render template for {filename}"))?;
    ui.action("Rendered", format!("template to {}", output_path.display()));

    let link_path = ctx.resolve_symlink_target(&target_str)?;
    ctx.validate_link_boundary(&filename, &link_path)?;

    dotfiles::ensure_symlink(&output_path, &link_path)
        .with_context(|| format!("Failed to create symlink for {filename}"))?;
    ui.action(
        "Linking",
        format!("{} -> {}", link_path.display(), output_path.display()),
    );

    ui.finished(format!(
        "{} is now managed by dotf",
        ui.highlight(&filename)
    ));

    Ok(())
}
