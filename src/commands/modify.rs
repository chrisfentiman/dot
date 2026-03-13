use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Select, theme::ColorfulTheme};

use crate::dotfiles;
use crate::dotfiles::DotfContext;
use crate::runner::Runner;

pub fn run(runner: &dyn Runner, ctx: &DotfContext, name: Option<String>) -> Result<()> {
    let symlinks = ctx.read_symlinks()?;

    let config_name = match name {
        Some(n) => n,
        None => {
            let mut names: Vec<String> = symlinks.symlinks.keys().cloned().collect();
            names.sort();
            if names.is_empty() {
                anyhow::bail!("No managed configs found. Run `dotf config <path>` to add one.");
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

    let configs_dir = ctx.configs_dir()?;
    let template_path = configs_dir.join(format!("{config_name}.tmpl"));

    if !template_path.exists() {
        anyhow::bail!("Template not found: {}", template_path.display());
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let template_str = template_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Template path is not valid UTF-8"))?;

    let result = runner
        .run(&editor, &[template_str], None)
        .with_context(|| format!("Failed to open editor: {editor}"))?;

    if !result.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    let secrets = ctx.read_secrets()?;
    let output_path = configs_dir.join(&config_name);
    dotfiles::render_and_write(&template_path, &output_path, &secrets)
        .with_context(|| format!("Failed to re-render {config_name}"))?;
    println!("{} Re-rendered {}", "✓".green(), config_name);

    if let Some(target_str) = symlinks.symlinks.get(&config_name) {
        let link_path = ctx.resolve_symlink_target(target_str)?;
        dotfiles::ensure_symlink(&output_path, &link_path)
            .with_context(|| format!("Failed to update symlink for {config_name}"))?;
        println!(
            "{} Symlink up to date: {} -> {}",
            "✓".green(),
            link_path.display(),
            output_path.display()
        );
    }

    println!("{} {} updated", "✓".green().bold(), config_name.cyan());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotfiles::SymlinksFile;
    use crate::runner::MockRunner;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn modify_editor_failure_returns_error() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();

        let _home = crate::EnvGuard::set("HOME", tmp.path().to_str().unwrap());
        let _editor = crate::EnvGuard::set("EDITOR", "false-editor");

        let dotfiles_dir = tmp.path().join("dotfiles");
        std::fs::create_dir_all(dotfiles_dir.join("configs")).unwrap();
        std::fs::write(dotfiles_dir.join("configs/gitconfig.tmpl"), "key = value").unwrap();

        let mut map = HashMap::new();
        map.insert("gitconfig".to_string(), "~/.gitconfig".to_string());
        let sf = SymlinksFile { symlinks: map };
        std::fs::write(
            dotfiles_dir.join(".symlinks.toml"),
            toml::to_string_pretty(&sf).unwrap(),
        )
        .unwrap();

        let tmpl_path = dotfiles_dir.join("configs/gitconfig.tmpl");
        let runner =
            MockRunner::new().on("false-editor", &[tmpl_path.to_str().unwrap()], "", false);

        let ctx = DotfContext::global();
        let err = run(&runner, &ctx, Some("gitconfig".to_string())).unwrap_err();
        assert!(err.to_string().contains("Editor exited"));
    }
}
