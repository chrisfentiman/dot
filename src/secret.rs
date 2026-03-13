use anyhow::{Result, anyhow};
use std::process::Command;

/// Fetch a secret value from any supported backend based on URI scheme.
///
/// Supported schemes:
///   pass://vault/item/field   — Proton Pass CLI (`pass`)
///   op://vault/item/field     — 1Password CLI (`op`)
///   bw://item-name/field      — Bitwarden CLI (`bw`, requires BW_SESSION env var)
///   env://VAR_NAME            — environment variable (useful for CI)
pub fn fetch(uri: &str) -> Result<String> {
    if let Some(path) = uri.strip_prefix("pass://") {
        fetch_pass(path, uri)
    } else if let Some(path) = uri.strip_prefix("op://") {
        fetch_op(path, uri)
    } else if let Some(path) = uri.strip_prefix("bw://") {
        fetch_bw(path, uri)
    } else if let Some(var) = uri.strip_prefix("env://") {
        fetch_env(var)
    } else {
        Err(anyhow!(
            "Unknown secret URI scheme: '{}'\n  Supported: pass://, op://, bw://, env://",
            uri
        ))
    }
}

/// Return a human-readable backend name for a URI.
pub fn backend_name(uri: &str) -> &'static str {
    if uri.starts_with("pass://") {
        "Proton Pass"
    } else if uri.starts_with("op://") {
        "1Password"
    } else if uri.starts_with("bw://") {
        "Bitwarden"
    } else if uri.starts_with("env://") {
        "environment"
    } else {
        "unknown"
    }
}

// pass item get pass://vault/item --fields password
fn fetch_pass(path: &str, original_uri: &str) -> Result<String> {
    let full_uri = format!("pass://{path}");
    let output = Command::new("pass")
        .args(["item", "get", &full_uri, "--fields", "password"])
        .output()
        .map_err(|e| anyhow!("Failed to run Proton Pass CLI (`pass`): {e}\n  Install: brew install protonpass/pass/pass"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Proton Pass failed for {original_uri}: {stderr}"));
    }

    let value = String::from_utf8(output.stdout)
        .map_err(|_| anyhow!("Proton Pass output for {original_uri} is not valid UTF-8"))?;
    Ok(value.trim().to_string())
}

// op read op://vault/item/field
fn fetch_op(path: &str, original_uri: &str) -> Result<String> {
    let full_uri = format!("op://{path}");
    let output = Command::new("op")
        .args(["read", &full_uri])
        .output()
        .map_err(|e| {
            anyhow!(
                "Failed to run 1Password CLI (`op`): {e}\n  Install: brew install 1password-cli"
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("1Password failed for {original_uri}: {stderr}"));
    }

    let value = String::from_utf8(output.stdout)
        .map_err(|_| anyhow!("1Password output for {original_uri} is not valid UTF-8"))?;
    Ok(value.trim().to_string())
}

// bw get password <item-name>  (field after last / if present, else "password")
fn fetch_bw(path: &str, original_uri: &str) -> Result<String> {
    // bw://item-name/field or bw://item-name
    let (item, field) = match path.rsplit_once('/') {
        Some((item, field)) => (item, field),
        None => (path, "password"),
    };

    let subcommand = match field {
        "username" => "username",
        "notes" => "notes",
        "uri" => "uri",
        _ => "password",
    };

    let output = Command::new("bw")
        .args(["get", subcommand, item])
        .output()
        .map_err(|e| anyhow!("Failed to run Bitwarden CLI (`bw`): {e}\n  Install: brew install bitwarden-cli\n  Requires BW_SESSION env var (run: export BW_SESSION=$(bw unlock --raw))"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Bitwarden failed for {original_uri}: {stderr}"));
    }

    let value = String::from_utf8(output.stdout)
        .map_err(|_| anyhow!("Bitwarden output for {original_uri} is not valid UTF-8"))?;
    Ok(value.trim().to_string())
}

fn fetch_env(var: &str) -> Result<String> {
    std::env::var(var).map_err(|_| anyhow!("Environment variable '{}' is not set", var))
}
