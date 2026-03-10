use anyhow::{anyhow, Context, Result};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| "Failed to parse .secrets.toml")
}

pub fn write_secrets(secrets: &SecretsFile) -> Result<()> {
    let path = secrets_toml_path()?;
    let content = toml::to_string_pretty(secrets)
        .with_context(|| "Failed to serialize .secrets.toml")?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write {}", path.display()))
}

pub fn read_symlinks() -> Result<SymlinksFile> {
    let path = symlinks_toml_path()?;
    if !path.exists() {
        return Ok(SymlinksFile::default());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| "Failed to parse .symlinks.toml")
}

pub fn write_symlinks(symlinks: &SymlinksFile) -> Result<()> {
    let path = symlinks_toml_path()?;
    let content = toml::to_string_pretty(symlinks)
        .with_context(|| "Failed to serialize .symlinks.toml")?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write {}", path.display()))
}

pub fn fetch_secret(uri: &str) -> Result<String> {
    let output = Command::new("pass")
        .args(["item", "get", uri, "--fields", "password"])
        .output()
        .with_context(|| format!("Failed to run pass for URI: {uri}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("pass failed for {uri}: {stderr}"));
    }

    let value = String::from_utf8(output.stdout)
        .with_context(|| "pass output is not valid UTF-8")?;
    Ok(value.trim().to_string())
}

pub fn render_template(template_path: &Path, secrets: &SecretsFile) -> Result<String> {
    let template_content = fs::read_to_string(template_path)
        .with_context(|| format!("Failed to read template {}", template_path.display()))?;

    let mut hbs = Handlebars::new();
    hbs.set_strict_mode(false);
    hbs.register_template_string("t", &template_content)
        .with_context(|| format!("Failed to parse template {}", template_path.display()))?;

    let mut values: HashMap<String, String> = HashMap::new();
    for (name, uri) in &secrets.secrets {
        match fetch_secret(uri) {
            Ok(val) => {
                values.insert(name.clone(), val);
            }
            Err(e) => {
                eprintln!("Warning: could not fetch secret {name} ({uri}): {e}");
            }
        }
    }

    let rendered = hbs
        .render("t", &values)
        .with_context(|| format!("Failed to render template {}", template_path.display()))?;
    Ok(rendered)
}

pub fn render_and_write(template_path: &Path, output_path: &Path, secrets: &SecretsFile) -> Result<()> {
    let rendered = render_template(template_path, secrets)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create dir {}", parent.display()))?;
    }
    fs::write(output_path, rendered)
        .with_context(|| format!("Failed to write rendered file {}", output_path.display()))
}

pub fn ensure_symlink(target: &Path, link: &Path) -> Result<()> {
    if link.exists() || link.symlink_metadata().is_ok() {
        let existing = fs::read_link(link).unwrap_or_default();
        if existing == target {
            return Ok(());
        }
        fs::remove_file(link)
            .with_context(|| format!("Failed to remove existing file/symlink at {}", link.display()))?;
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent dir {}", parent.display()))?;
    }
    unix_fs::symlink(target, link)
        .with_context(|| format!("Failed to create symlink {} -> {}", link.display(), target.display()))
}

pub fn render_and_symlink_all() -> Result<Vec<String>> {
    let secrets = read_secrets()?;
    let symlinks = read_symlinks()?;
    let configs = configs_dir()?;
    let mut done = Vec::new();

    for (name, target_str) in &symlinks.symlinks {
        let template_path = configs.join(format!("{name}.tmpl"));
        let output_path = configs.join(name);
        let link_path = expand_tilde(target_str)?;

        if !template_path.exists() {
            eprintln!("Warning: template not found for {name}, skipping");
            continue;
        }

        render_and_write(&template_path, &output_path, &secrets)
            .with_context(|| format!("Failed to render {name}"))?;

        ensure_symlink(&output_path, &link_path)
            .with_context(|| format!("Failed to symlink {name}"))?;

        done.push(format!("{name} -> {target_str}"));
    }

    Ok(done)
}
