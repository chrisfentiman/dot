use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use std::fs;

use crate::dotfiles;

pub fn run(name: Option<String>) -> Result<()> {
    let mut symlinks = dotfiles::read_symlinks()?;

    let config_name = match name {
        Some(n) => {
            if !symlinks.symlinks.contains_key(&n) {
                anyhow::bail!("Config '{}' is not managed by dotf", n);
            }
            n
        }
        None => {
            if symlinks.symlinks.is_empty() {
                anyhow::bail!("No managed configs found.");
            }
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

    // Remove symlink — handle NotFound gracefully (race between check and delete).
    match fs::remove_file(&link_path) {
        Ok(()) => println!("{} Removed symlink {}", "✓".green(), link_path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(
                anyhow::Error::new(e)
                    .context(format!("Failed to remove symlink {}", link_path.display())),
            );
        }
    }

    // Optionally restore the rendered file in place of the symlink
    if restore && rendered_path.exists() {
        fs::copy(&rendered_path, &link_path)
            .with_context(|| format!("Failed to restore file to {}", link_path.display()))?;
        // Restore user-readable permissions (rendered files are 0o600, but
        // user-facing configs like .gitconfig should be 0o644).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&link_path, fs::Permissions::from_mode(0o644));
        }
        println!("{} Restored file to {}", "✓".green(), link_path.display());
    }

    // Remove template
    match fs::remove_file(&template_path) {
        Ok(()) => println!("{} Removed template {}", "✓".green(), template_path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(
                anyhow::Error::new(e)
                    .context(format!("Failed to remove {}", template_path.display())),
            );
        }
    }

    // Remove rendered output
    match fs::remove_file(&rendered_path) {
        Ok(()) => println!("{} Removed rendered file {}", "✓".green(), rendered_path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(
                anyhow::Error::new(e)
                    .context(format!("Failed to remove {}", rendered_path.display())),
            );
        }
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

#[cfg(test)]
mod tests {
    use crate::dotfiles::SymlinksFile;
    use std::collections::HashMap;
    use tempfile::TempDir;

    struct Env {
        _tmp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new() -> Self {
            let _lock = crate::env_lock();
            let tmp = TempDir::new().unwrap();
            let dotfiles = tmp.path().join("dotfiles");
            std::fs::create_dir_all(dotfiles.join("configs")).unwrap();
            unsafe { std::env::set_var("HOME", tmp.path()); }
            Env { _tmp: tmp, _lock }
        }

        fn dotfiles(&self) -> std::path::PathBuf {
            self._tmp.path().join("dotfiles")
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            unsafe { std::env::remove_var("HOME"); }
        }
    }

    #[test]
    fn remove_unknown_config_errors() {
        let env = Env::new();
        let _ = &env;
        // Write an empty symlinks file
        let sf = SymlinksFile { symlinks: HashMap::new() };
        let path = env.dotfiles().join(".symlinks.toml");
        std::fs::write(&path, toml::to_string_pretty(&sf).unwrap()).unwrap();

        let err = super::run(Some("nonexistent".into())).unwrap_err();
        assert!(
            err.to_string().contains("nonexistent"),
            "should name the missing config: {}",
            err
        );
    }

    #[test]
    fn remove_named_config_from_empty_errors_with_name() {
        let env = Env::new();
        let _ = &env;
        let err = super::run(Some("cfg".into())).unwrap_err();
        assert!(
            err.to_string().contains("cfg"),
            "should name the missing config: {}",
            err
        );
    }
}
