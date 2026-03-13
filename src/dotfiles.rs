use anyhow::{Context, Result, anyhow};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::secret;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SecretsFile {
    pub secrets: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SymlinksFile {
    pub symlinks: HashMap<String, String>,
}

pub fn dotfiles_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    Ok(home.join("dotfiles"))
}

pub fn configs_dir() -> Result<PathBuf> {
    Ok(dotfiles_dir()?.join("configs"))
}

pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
        Ok(home.join(rest))
    } else if path == "~" {
        dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))
    } else {
        Ok(PathBuf::from(path))
    }
}

pub fn secrets_toml_path() -> Result<PathBuf> {
    Ok(dotfiles_dir()?.join(".secrets.toml"))
}

pub fn symlinks_toml_path() -> Result<PathBuf> {
    Ok(dotfiles_dir()?.join(".symlinks.toml"))
}

pub fn read_secrets() -> Result<SecretsFile> {
    let path = secrets_toml_path()?;
    if !path.exists() {
        return Ok(SecretsFile::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| "Failed to parse .secrets.toml")
}

pub fn write_secrets(secrets: &SecretsFile) -> Result<()> {
    let path = secrets_toml_path()?;
    let content =
        toml::to_string_pretty(secrets).with_context(|| "Failed to serialize .secrets.toml")?;
    atomic_write(&path, content.as_bytes(), 0o600)
        .with_context(|| format!("Failed to write {}", path.display()))
}

pub fn read_symlinks() -> Result<SymlinksFile> {
    let path = symlinks_toml_path()?;
    if !path.exists() {
        return Ok(SymlinksFile::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| "Failed to parse .symlinks.toml")
}

pub fn write_symlinks(symlinks: &SymlinksFile) -> Result<()> {
    let path = symlinks_toml_path()?;
    let content =
        toml::to_string_pretty(symlinks).with_context(|| "Failed to serialize .symlinks.toml")?;
    atomic_write(&path, content.as_bytes(), 0o644)
        .with_context(|| format!("Failed to write {}", path.display()))
}

pub fn fetch_secret(uri: &str) -> Result<Zeroizing<String>> {
    secret::fetch(uri)
}

pub fn render_template(template_path: &Path, secrets: &SecretsFile) -> Result<String> {
    let template_content = fs::read_to_string(template_path)
        .with_context(|| format!("Failed to read template {}", template_path.display()))?;

    let mut hbs = Handlebars::new();
    hbs.set_strict_mode(false);
    hbs.register_template_string("t", &template_content)
        .with_context(|| format!("Failed to parse template {}", template_path.display()))?;

    let mut values: HashMap<String, String> = HashMap::new();
    let mut failed: Vec<String> = Vec::new();
    for (name, uri) in &secrets.secrets {
        match fetch_secret(uri) {
            Ok(val) => {
                // `val` is a `Zeroizing<String>` — clone the inner str into the map
                // and let `val` drop (and zero) at end of this block.
                values.insert(name.clone(), val.as_str().to_string());
            }
            Err(e) => {
                failed.push(format!("{name} ({uri}): {e}"));
            }
        }
    }
    if !failed.is_empty() {
        anyhow::bail!(
            "Failed to fetch {} secret(s):\n  {}",
            failed.len(),
            failed.join("\n  ")
        );
    }

    let rendered = hbs
        .render("t", &values)
        .with_context(|| format!("Failed to render template {}", template_path.display()))?;
    Ok(rendered)
}

/// Write `data` to `path` atomically (write to a tempfile then rename) and
/// set Unix permissions to `mode` (e.g. `0o600` for owner-only read/write).
/// After the rename, the parent directory is synced to make the new directory
/// entry durable across a crash.
pub fn atomic_write(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create dir {}", parent.display()))?;

    let tmp = tempfile::Builder::new()
        .tempfile_in(parent)
        .with_context(|| format!("Failed to create tempfile in {}", parent.display()))?;

    // Set permissions before writing data.
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))
        .with_context(|| "Failed to set permissions on tempfile")?;

    fs::write(tmp.path(), data)
        .with_context(|| format!("Failed to write to tempfile for {}", path.display()))?;

    tmp.persist(path)
        .with_context(|| format!("Failed to persist tempfile to {}", path.display()))?;

    // Sync the parent directory so the rename is durable on crash.
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }

    Ok(())
}

pub fn render_and_write(
    template_path: &Path,
    output_path: &Path,
    secrets: &SecretsFile,
) -> Result<()> {
    let rendered = render_template(template_path, secrets)?;
    atomic_write(output_path, rendered.as_bytes(), 0o600)
        .with_context(|| format!("Failed to write rendered file {}", output_path.display()))
}

pub fn ensure_symlink(target: &Path, link: &Path) -> Result<()> {
    if link.exists() || link.symlink_metadata().is_ok() {
        let existing = fs::read_link(link).unwrap_or_default();
        if existing == target {
            return Ok(());
        }
        fs::remove_file(link).with_context(|| {
            format!(
                "Failed to remove existing file/symlink at {}",
                link.display()
            )
        })?;
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent dir {}", parent.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs as unix_fs;
        unix_fs::symlink(target, link).with_context(|| {
            format!(
                "Failed to create symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs as win_fs;
        win_fs::symlink_file(target, link).with_context(|| {
            format!(
                "Failed to create symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
}

/// Exposed for testing only — splits bw URI into (item, subcommand).
#[cfg(test)]
pub fn _bw_parse(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((item, field)) => {
            let sub = match field {
                "username" => "username",
                "notes" => "notes",
                "uri" => "uri",
                _ => "password",
            };
            (item, sub)
        }
        None => (path, "password"),
    }
}

pub fn render_and_symlink_all() -> Result<Vec<String>> {
    let secrets = read_secrets()?;
    let symlinks = read_symlinks()?;
    let configs = configs_dir()?;
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    let mut done = Vec::new();

    for (name, target_str) in &symlinks.symlinks {
        let template_path = configs.join(format!("{name}.tmpl"));
        let output_path = configs.join(name);
        let link_path = expand_tilde(target_str)?;

        if !template_path.exists() {
            eprintln!("Warning: template not found for {name}, skipping");
            continue;
        }

        // Validate the symlink destination stays inside HOME.
        if !link_path.starts_with(&home) {
            anyhow::bail!(
                "Refusing to symlink {name}: destination {} is outside home directory",
                link_path.display()
            );
        }

        render_and_write(&template_path, &output_path, &secrets)
            .with_context(|| format!("Failed to render {name}"))?;

        ensure_symlink(&output_path, &link_path)
            .with_context(|| format!("Failed to symlink {name}"))?;

        done.push(format!("{name} -> {target_str}"));
    }

    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    // ── expand_tilde ─────────────────────────────────────────────
    #[test]
    fn expand_tilde_plain_path() {
        let p = expand_tilde("/etc/hosts").unwrap();
        assert_eq!(p, PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn expand_tilde_tilde_only() {
        let p = expand_tilde("~").unwrap();
        assert_eq!(p, dirs::home_dir().unwrap());
    }

    #[test]
    fn expand_tilde_tilde_slash() {
        let p = expand_tilde("~/.gitconfig").unwrap();
        assert_eq!(p, dirs::home_dir().unwrap().join(".gitconfig"));
    }

    #[test]
    fn expand_tilde_nested_path() {
        let p = expand_tilde("~/.config/mise/config.toml").unwrap();
        assert_eq!(
            p,
            dirs::home_dir().unwrap().join(".config/mise/config.toml")
        );
    }

    #[test]
    fn expand_tilde_no_tilde() {
        let p = expand_tilde("relative/path").unwrap();
        assert_eq!(p, PathBuf::from("relative/path"));
    }

    // ── render_template ──────────────────────────────────────────
    #[test]
    fn render_template_substitutes_env_placeholders() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "email = {{EMAIL}}\ntoken = {{TOKEN}}").unwrap();

        unsafe {
            std::env::set_var("_DOTF_T_EMAIL", "chris@example.com");
        }
        unsafe {
            std::env::set_var("_DOTF_T_TOKEN", "abc123");
        }

        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("EMAIL".to_string(), "env://_DOTF_T_EMAIL".to_string()),
                ("TOKEN".to_string(), "env://_DOTF_T_TOKEN".to_string()),
            ]),
        };

        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "email = chris@example.com\ntoken = abc123");

        unsafe {
            std::env::remove_var("_DOTF_T_EMAIL");
        }
        unsafe {
            std::env::remove_var("_DOTF_T_TOKEN");
        }
    }

    #[test]
    fn render_template_unknown_placeholder_renders_empty() {
        // handlebars in non-strict mode renders missing keys as empty string
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "name = {{MISSING}}").unwrap();

        let secrets = SecretsFile::default();
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "name = ");
    }

    #[test]
    fn render_template_no_placeholders_is_identity() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        let content = "[core]\n  editor = nvim\n  autocrlf = input\n";
        fs::write(&tmpl, content).unwrap();

        let rendered = render_template(&tmpl, &SecretsFile::default()).unwrap();
        assert_eq!(rendered, content);
    }

    #[test]
    fn render_template_multiline_value() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "key = {{VAL}}").unwrap();

        unsafe {
            std::env::set_var("_DOTF_T_VAL", "line1\nline2");
        }
        let secrets = SecretsFile {
            secrets: HashMap::from([("VAL".to_string(), "env://_DOTF_T_VAL".to_string())]),
        };

        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "key = line1\nline2");
        unsafe {
            std::env::remove_var("_DOTF_T_VAL");
        }
    }

    // ── ensure_symlink ───────────────────────────────────────────
    #[cfg(unix)]
    #[test]
    fn ensure_symlink_creates_link() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        let link = tmp.path().join("link.conf");

        fs::write(&target, "contents").unwrap();
        ensure_symlink(&target, &link).unwrap();

        assert!(link.symlink_metadata().is_ok());
        assert_eq!(fs::read_link(&link).unwrap(), target);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_noop_if_already_correct() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        let link = tmp.path().join("link.conf");

        fs::write(&target, "contents").unwrap();
        ensure_symlink(&target, &link).unwrap();
        // Call again — should not error
        ensure_symlink(&target, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), target);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_replaces_stale_link() {
        let tmp = TempDir::new().unwrap();
        let old_target = tmp.path().join("old.conf");
        let new_target = tmp.path().join("new.conf");
        let link = tmp.path().join("link.conf");

        fs::write(&old_target, "old").unwrap();
        fs::write(&new_target, "new").unwrap();

        ensure_symlink(&old_target, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), old_target);

        ensure_symlink(&new_target, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), new_target);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        let link = tmp.path().join("deep/nested/dir/link.conf");

        fs::write(&target, "contents").unwrap();
        ensure_symlink(&target, &link).unwrap();
        assert!(link.symlink_metadata().is_ok());
    }

    // ── render_and_write ─────────────────────────────────────────
    #[test]
    fn render_and_write_creates_output_file() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("cfg.tmpl");
        let out = tmp.path().join("cfg");

        unsafe {
            std::env::set_var("_DOTF_T_HOST", "myhost");
        }
        fs::write(&tmpl, "host = {{HOST}}").unwrap();

        let secrets = SecretsFile {
            secrets: HashMap::from([("HOST".to_string(), "env://_DOTF_T_HOST".to_string())]),
        };

        render_and_write(&tmpl, &out, &secrets).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "host = myhost");
        unsafe {
            std::env::remove_var("_DOTF_T_HOST");
        }
    }
}
