use anyhow::{Context, Result};
use clap::Subcommand;

use crate::dotfiles::DotfContext;
use crate::ui::UI;
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

pub fn run(ui: &UI, ctx: &DotfContext, action: SecretsAction) -> Result<()> {
    match action {
        SecretsAction::List => list(ui, ctx),
        SecretsAction::Validate => validate(ui, ctx),
        SecretsAction::Add { name, uri } => add(ui, ctx, name, uri),
        SecretsAction::Remove { name } => remove(ui, ctx, name),
    }
}

fn list(ui: &UI, ctx: &DotfContext) -> Result<()> {
    let secrets = ctx.read_secrets()?;

    if secrets.secrets.is_empty() {
        ui.action(
            "Secrets",
            format!(
                "none configured — run {} to add one",
                ui.highlight("dotf secrets add <name> <uri>")
            ),
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

    ui.table_header(&[
        ("PLACEHOLDER", name_width),
        ("SECRET URI", uri_width),
        ("BACKEND", backend_width),
    ]);
    ui.table_separator(name_width + uri_width + backend_width + 4);

    let mut entries: Vec<_> = secrets.secrets.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    for (name, uri) in entries {
        ui.table_row(format!(
            "{:<name_width$}  {:<uri_width$}  {}",
            ui.highlight(name),
            uri,
            ui.dim(secret::backend_name(uri))
        ));
    }

    Ok(())
}

fn validate(ui: &UI, ctx: &DotfContext) -> Result<()> {
    let secrets = ctx.read_secrets()?;

    if secrets.secrets.is_empty() {
        ui.action("Validate", "no secrets configured");
        return Ok(());
    }

    let mut passed = 0usize;
    let mut failed = 0usize;

    let mut entries: Vec<_> = secrets.secrets.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    for (name, uri) in entries {
        match secret::fetch(uri) {
            Ok(_) => {
                ui.raw(format!(
                    "{} {} {}",
                    ui.sym_ok(),
                    ui.highlight(name),
                    ui.dim(format!("({})", uri))
                ));
                passed += 1;
            }
            Err(e) => {
                ui.raw(format!(
                    "{} {} {} — {}",
                    ui.sym_err(),
                    ui.highlight(name),
                    ui.dim(format!("({})", uri)),
                    e
                ));
                failed += 1;
            }
        }
    }

    ui.blank();
    ui.raw(format!(
        "{} passed, {} failed",
        ui.bold(passed),
        if failed > 0 {
            ui.bold(failed)
        } else {
            ui.dim(failed)
        }
    ));

    if failed > 0 {
        anyhow::bail!("{failed} secret(s) failed validation");
    }

    Ok(())
}

fn add(ui: &UI, ctx: &DotfContext, name: String, uri: String) -> Result<()> {
    if !dotfiles::is_valid_placeholder_name(&name) {
        anyhow::bail!(
            "Invalid placeholder name '{}': must be non-empty and contain only ASCII alphanumeric characters and underscores",
            name
        );
    }
    if !dotfiles::is_valid_secret_uri(&uri) {
        anyhow::bail!(
            "Invalid secret URI '{}': must start with pass://, op://, bw://, or env://",
            uri
        );
    }
    let mut secrets = ctx.read_secrets()?;
    let existed = secrets.secrets.contains_key(&name);
    secrets.secrets.insert(name.clone(), uri.clone());
    ctx.write_secrets(&secrets)
        .context("Failed to write .secrets.toml")?;

    if existed {
        ui.action("Updated", format!("{} -> {}", ui.highlight(&name), uri));
    } else {
        ui.action("Added", format!("{} -> {}", ui.highlight(&name), uri));
    }
    Ok(())
}

fn remove(ui: &UI, ctx: &DotfContext, name: String) -> Result<()> {
    let mut secrets = ctx.read_secrets()?;
    if secrets.secrets.remove(&name).is_none() {
        anyhow::bail!("Secret '{}' not found in .secrets.toml", name);
    }
    ctx.write_secrets(&secrets)
        .context("Failed to write .secrets.toml")?;
    ui.action("Removed", ui.highlight(&name));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    }

    fn ctx() -> DotfContext {
        DotfContext::global()
    }

    // ── add ──────────────────────────────────────────────────────
    #[test]
    fn add_inserts_new_secret() {
        let _e = Env::new();
        let ui = UI::new();
        add(&ui, &ctx(), "FOO".into(), "env://FOO".into()).unwrap();
        let s = ctx().read_secrets().unwrap();
        assert_eq!(s.secrets["FOO"], "env://FOO");
    }

    #[test]
    fn add_overwrites_existing_secret() {
        let _e = Env::new();
        let ui = UI::new();
        add(&ui, &ctx(), "FOO".into(), "env://OLD".into()).unwrap();
        add(&ui, &ctx(), "FOO".into(), "env://NEW".into()).unwrap();
        let s = ctx().read_secrets().unwrap();
        assert_eq!(s.secrets["FOO"], "env://NEW");
    }

    // ── remove ───────────────────────────────────────────────────
    #[test]
    fn remove_deletes_existing_secret() {
        let _e = Env::new();
        let ui = UI::new();
        add(&ui, &ctx(), "BAR".into(), "env://BAR".into()).unwrap();
        remove(&ui, &ctx(), "BAR".into()).unwrap();
        let s = ctx().read_secrets().unwrap();
        assert!(!s.secrets.contains_key("BAR"));
    }

    #[test]
    fn remove_errors_on_missing_secret() {
        let _e = Env::new();
        let ui = UI::new();
        let err = remove(&ui, &ctx(), "NOPE".into()).unwrap_err();
        assert!(err.to_string().contains("NOPE"));
    }

    // ── validate ─────────────────────────────────────────────────
    #[test]
    fn validate_passes_when_env_secrets_present() {
        let _e = Env::new();
        let ui = UI::new();
        let _val = crate::EnvGuard::set("_DOTF_TEST_VAL", "value");
        add(&ui, &ctx(), "VAL".into(), "env://_DOTF_TEST_VAL".into()).unwrap();
        validate(&ui, &ctx()).unwrap();
    }

    #[test]
    fn validate_fails_when_secret_missing() {
        let _e = Env::new();
        let ui = UI::new();
        unsafe {
            std::env::remove_var("_DOTF_TEST_ABSENT");
        }
        add(
            &ui,
            &ctx(),
            "ABSENT".into(),
            "env://_DOTF_TEST_ABSENT".into(),
        )
        .unwrap();
        let err = validate(&ui, &ctx()).unwrap_err();
        assert!(err.to_string().contains("failed validation"));
    }

    #[test]
    fn validate_empty_secrets_returns_ok() {
        let _e = Env::new();
        let ui = UI::new();
        validate(&ui, &ctx()).unwrap();
    }

    // ── add validation ──────────────────────────────────────────
    #[test]
    fn add_rejects_invalid_placeholder_name() {
        let _e = Env::new();
        let ui = UI::new();
        let err = add(&ui, &ctx(), "invalid-name".into(), "env://FOO".into()).unwrap_err();
        assert!(err.to_string().contains("Invalid placeholder name"));
    }

    #[test]
    fn add_rejects_invalid_uri_scheme() {
        let _e = Env::new();
        let ui = UI::new();
        let err = add(&ui, &ctx(), "FOO".into(), "https://example.com".into()).unwrap_err();
        assert!(err.to_string().contains("Invalid secret URI"));
    }
}
