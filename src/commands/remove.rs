use anyhow::{Context, Result};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use std::fs;

use crate::dotfiles::DotfContext;
use crate::ui::UI;

pub fn run(ui: &UI, ctx: &DotfContext, name: Option<String>) -> Result<()> {
    let mut symlinks = ctx.read_symlinks()?;

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

    let target_str = symlinks
        .symlinks
        .get(&config_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Config '{}' not found in .symlinks.toml", config_name))?;
    let configs_dir = ctx.configs_dir()?;
    let template_path = configs_dir.join(format!("{config_name}.tmpl"));
    let rendered_path = configs_dir.join(&config_name);
    let link_path = ctx.resolve_symlink_target(&target_str)?;

    ui.blank();
    ui.raw("This will:");
    ui.raw(format!(
        "  {} Remove symlink {}",
        ui.sym_dim(),
        link_path.display()
    ));
    ui.raw(format!(
        "  {} Remove template {}",
        ui.sym_dim(),
        template_path.display()
    ));
    ui.raw(format!(
        "  {} Remove rendered {}",
        ui.sym_dim(),
        rendered_path.display()
    ));
    ui.raw(format!("  {} Remove from .symlinks.toml", ui.sym_dim()));
    ui.blank();

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
        ui.skip("Aborted", "removal cancelled");
        return Ok(());
    }

    // Remove symlink — handle NotFound gracefully (race between check and delete).
    match fs::remove_file(&link_path) {
        Ok(()) => ui.action("Removed", format!("symlink {}", link_path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("Failed to remove symlink {}", link_path.display())));
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
        ui.action("Restored", format!("file to {}", link_path.display()));
    }

    // Remove template
    match fs::remove_file(&template_path) {
        Ok(()) => ui.action("Removed", format!("template {}", template_path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("Failed to remove {}", template_path.display())));
        }
    }

    // Remove rendered output
    match fs::remove_file(&rendered_path) {
        Ok(()) => ui.action(
            "Removed",
            format!("rendered file {}", rendered_path.display()),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("Failed to remove {}", rendered_path.display())));
        }
    }

    // Remove from .symlinks.toml
    symlinks.symlinks.remove(&config_name);
    ctx.write_symlinks(&symlinks)
        .context("Failed to update .symlinks.toml")?;
    ui.action("Removed", "from .symlinks.toml");

    ui.finished(format!(
        "'{}' is no longer managed by dotf",
        ui.highlight(&config_name)
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::dotfiles::{DotfContext, SymlinksFile};
    use std::collections::HashMap;
    use tempfile::TempDir;

    struct Env {
        // Drop order: _home_guard restores HOME, _tmp deletes dir, _lock releases mutex.
        _home_guard: crate::EnvGuard,
        _tmp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new() -> Self {
            let _lock = crate::env_lock();
            let tmp = TempDir::new().unwrap();
            let dotfiles = tmp.path().join(".dotf");
            std::fs::create_dir_all(dotfiles.join("configs")).unwrap();
            let _home_guard = crate::EnvGuard::set("HOME", &tmp.path().to_string_lossy());
            Env {
                _tmp: tmp,
                _home_guard,
                _lock,
            }
        }

        fn dotfiles(&self) -> std::path::PathBuf {
            self._tmp.path().join(".dotf")
        }
    }

    fn ctx() -> DotfContext {
        DotfContext::global()
    }

    #[test]
    fn remove_unknown_config_errors() {
        let env = Env::new();
        let _ = &env;
        let sf = SymlinksFile {
            symlinks: HashMap::new(),
        };
        let path = env.dotfiles().join(".symlinks.toml");
        std::fs::write(&path, toml::to_string_pretty(&sf).unwrap()).unwrap();

        let ui = crate::ui::UI::new();
        let err = super::run(&ui, &ctx(), Some("nonexistent".into())).unwrap_err();
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
        let ui = crate::ui::UI::new();
        let err = super::run(&ui, &ctx(), Some("cfg".into())).unwrap_err();
        assert!(
            err.to_string().contains("cfg"),
            "should name the missing config: {}",
            err
        );
    }
}
