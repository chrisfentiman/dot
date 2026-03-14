use anyhow::{Context, Result, anyhow};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::secret;

// ── DotfContext ──────────────────────────────────────────────────────────────

/// Whether dotf operates on the global `~/.dotf` store or a project-local
/// `.dotf/` directory.
#[derive(Debug, Clone)]
pub enum DotfMode {
    /// Global mode: data lives at `~/.dotf`.
    Global,
    /// Local mode: data lives at `<project_root>/.dotf/`.
    Local(PathBuf),
}

/// Carries the resolved operating mode through all commands.
#[derive(Debug, Clone)]
pub struct DotfContext {
    pub mode: DotfMode,
}

impl DotfContext {
    /// Construct a global-mode context (the default).
    pub fn global() -> Self {
        Self {
            mode: DotfMode::Global,
        }
    }

    /// Construct a local-mode context rooted at `project_root`.
    /// The directory does not need to exist yet (init creates it).
    pub fn local(project_root: PathBuf) -> Self {
        Self {
            mode: DotfMode::Local(project_root),
        }
    }

    /// Construct a local-mode context from a user-provided path.
    ///
    /// Uses `std::path::absolute` (not `canonicalize`) because the path may
    /// not exist yet — `dotf init <path>` creates it.
    pub fn local_from_path(path: &Path) -> Result<Self> {
        let abs = std::path::absolute(path)
            .with_context(|| format!("invalid path '{}'", path.display()))?;
        Ok(Self::local(abs))
    }

    /// The base directory where dotf stores its data.
    ///
    /// - Global: `~/.dotf`
    /// - Local:  `<project_root>/.dotf`
    pub fn dotfiles_dir(&self) -> Result<PathBuf> {
        match &self.mode {
            DotfMode::Global => {
                let home =
                    dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
                Ok(home.join(".dotf"))
            }
            DotfMode::Local(root) => Ok(root.join(".dotf")),
        }
    }

    pub fn configs_dir(&self) -> Result<PathBuf> {
        Ok(self.dotfiles_dir()?.join("configs"))
    }

    pub fn secrets_toml_path(&self) -> Result<PathBuf> {
        Ok(self.dotfiles_dir()?.join(".secrets.toml"))
    }

    pub fn symlinks_toml_path(&self) -> Result<PathBuf> {
        Ok(self.dotfiles_dir()?.join(".symlinks.toml"))
    }

    pub fn read_secrets(&self) -> Result<SecretsFile> {
        read_toml_file(&self.secrets_toml_path()?)
    }

    pub fn write_secrets(&self, secrets: &SecretsFile) -> Result<()> {
        let path = self.secrets_toml_path()?;
        let content =
            toml::to_string_pretty(secrets).context("Failed to serialize .secrets.toml")?;
        atomic_write(&path, content.as_bytes(), 0o600)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    pub fn read_symlinks(&self) -> Result<SymlinksFile> {
        read_toml_file(&self.symlinks_toml_path()?)
    }

    pub fn write_symlinks(&self, symlinks: &SymlinksFile) -> Result<()> {
        let path = self.symlinks_toml_path()?;
        let content =
            toml::to_string_pretty(symlinks).context("Failed to serialize .symlinks.toml")?;
        atomic_write(&path, content.as_bytes(), 0o600)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    /// The root directory used as a security boundary for symlink targets.
    ///
    /// - Global: user's HOME directory
    /// - Local:  the project root
    pub fn root_dir(&self) -> Result<PathBuf> {
        match &self.mode {
            DotfMode::Global => {
                dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))
            }
            DotfMode::Local(root) => Ok(root.clone()),
        }
    }

    /// Resolve a symlink target string to an absolute path.
    ///
    /// - Global: expands `~/.gitconfig` → `/Users/chris/.gitconfig`
    /// - Local:  joins project root + relative path (rejects absolute paths)
    pub fn resolve_symlink_target(&self, target_str: &str) -> Result<PathBuf> {
        match &self.mode {
            DotfMode::Global => expand_tilde(target_str),
            DotfMode::Local(root) => {
                if target_str.starts_with('/') || target_str.starts_with('~') {
                    anyhow::bail!(
                        "Local mode symlink targets must be relative paths, got: {target_str}"
                    );
                }
                Ok(root.join(target_str))
            }
        }
    }

    /// Print a dimmed header to stderr showing the resolved mode and root.
    /// Uses stderr so stdout remains pipeable.
    pub fn print_mode_header(&self) {
        use colored::Colorize;
        match &self.mode {
            DotfMode::Global => eprintln!("{}", "dotf (global ~/.dotf)".dimmed()),
            DotfMode::Local(root) => {
                eprintln!("{}", format!("dotf (local {})", root.display()).dimmed())
            }
        }
        eprintln!();
    }

    /// Validate that `link_path` resolves to a location inside this context's
    /// root directory (HOME for global, project root for local). Bails with a
    /// descriptive error if the path escapes the boundary.
    pub fn validate_link_boundary(&self, name: &str, link_path: &Path) -> Result<()> {
        let root = self.root_dir()?;
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        let canonical_link = link_path.canonicalize().unwrap_or_else(|_| {
            let file_name = link_path.file_name().unwrap_or_default().to_string_lossy();
            if file_name.contains("..") {
                return link_path.to_path_buf();
            }
            link_path
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.join(file_name.as_ref()))
                .unwrap_or_else(|| link_path.to_path_buf())
        });
        if !canonical_link.starts_with(&canonical_root) {
            anyhow::bail!(
                "Refusing to symlink {name}: destination {} is outside {}",
                link_path.display(),
                match &self.mode {
                    DotfMode::Global => "home directory",
                    DotfMode::Local(_) => "project root",
                }
            );
        }
        Ok(())
    }

    /// Render all templates, write outputs, and create symlinks.
    pub fn render_and_symlink_all(&self) -> Result<Vec<String>> {
        let secrets = self.read_secrets()?;
        let symlinks = self.read_symlinks()?;
        let configs = self.configs_dir()?;
        let mut done = Vec::new();

        for (name, target_str) in &symlinks.symlinks {
            let template_path = configs.join(format!("{name}.tmpl"));
            let output_path = configs.join(name);
            let link_path = self.resolve_symlink_target(target_str)?;

            if !template_path.exists() {
                eprintln!("Warning: template not found for {name}, skipping");
                continue;
            }

            self.validate_link_boundary(name, &link_path)?;

            render_and_write(&template_path, &output_path, &secrets)
                .with_context(|| format!("Failed to render {name}"))?;

            ensure_symlink(&output_path, &link_path)
                .with_context(|| format!("Failed to symlink {name}"))?;

            done.push(format!("{name} -> {target_str}"));
        }

        Ok(done)
    }
}

/// Walk from `start` up to filesystem root looking for a `.dotf/` directory.
/// Returns the directory *containing* `.dotf/` (the project root), not `.dotf/` itself.
/// Returns `None` if no `.dotf/` is found before reaching the filesystem root.
///
/// On Unix, skips `.dotf/` directories not owned by the current user to mitigate
/// malicious directory injection (analogous to CVE-2022-24765 in Git).
pub fn find_dotf_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| {
            let dotf = dir.join(".dotf");
            dotf.is_dir() && is_owned_by_current_user(&dotf)
        })
        .map(Path::to_path_buf)
}

/// Check that a path is owned by the current user.
/// Always returns `true` on non-Unix platforms.
#[cfg(unix)]
fn is_owned_by_current_user(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match path.metadata() {
        Ok(meta) => meta.uid() == unsafe { libc::getuid() },
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_owned_by_current_user(_path: &Path) -> bool {
    true
}

/// Auto-detect the operating mode by walking up from the current directory.
///
/// - If a `.dotf/` directory is found at `$HOME`, use global mode.
/// - If a `.dotf/` directory is found elsewhere (closer to cwd), use local mode.
/// - If no `.dotf/` is found, default to global mode.
pub fn resolve_context() -> Result<DotfContext> {
    let cwd = std::env::current_dir().context("Cannot determine current directory")?;
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    Ok(resolve_context_from(&cwd, &home))
}

/// Core logic for scope auto-detection, separated from environment access for
/// testability. Determines Global vs Local based on where `.dotf/` is found
/// relative to `home`.
pub fn resolve_context_from(cwd: &Path, home: &Path) -> DotfContext {
    // Canonicalize to handle macOS firmlinks (/Users → /System/Volumes/Data/Users)
    // and other symlink scenarios where byte-level PathBuf comparison would fail.
    let canonical_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());

    match find_dotf_root(cwd) {
        Some(root) => {
            let canonical_root = root.canonicalize().unwrap_or(root);
            if canonical_root == canonical_home {
                DotfContext::global()
            } else {
                DotfContext::local(canonical_root)
            }
        }
        None => DotfContext::global(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SecretsFile {
    #[serde(deserialize_with = "deserialize_validated_secrets", default)]
    pub secrets: HashMap<String, String>,
}

/// Check that a name consists only of ASCII alphanumeric characters and underscores.
/// Shared between serde validation and the interactive `config` command.
pub fn is_valid_placeholder_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn is_valid_secret_uri(uri: &str) -> bool {
    uri.starts_with("pass://")
        || uri.starts_with("op://")
        || uri.starts_with("bw://")
        || uri.starts_with("env://")
}

fn deserialize_validated_secrets<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map = HashMap::<String, String>::deserialize(deserializer)?;
    for (key, uri) in &map {
        if !is_valid_placeholder_name(key) {
            return Err(serde::de::Error::custom(format!(
                "Invalid placeholder name '{}': must be non-empty and contain only ASCII alphanumeric characters and underscores",
                key
            )));
        }
        if !is_valid_secret_uri(uri) {
            return Err(serde::de::Error::custom(format!(
                "Invalid secret URI '{}' for placeholder '{}': must start with pass://, op://, bw://, or env://",
                uri, key
            )));
        }
    }
    Ok(map)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SymlinksFile {
    pub symlinks: HashMap<String, String>,
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

fn read_toml_file<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn fetch_secret(uri: &str) -> Result<Zeroizing<String>> {
    secret::fetch(uri)
}

pub fn render_template(template_path: &Path, secrets: &SecretsFile) -> Result<String> {
    let content = fs::read_to_string(template_path)
        .with_context(|| format!("Failed to read template {}", template_path.display()))?;

    render_template_str(&content, secrets)
        .with_context(|| format!("Failed to render template {}", template_path.display()))
}

/// Render a template from an in-memory string. This is the core single-pass
/// renderer, separated from `render_template` so it can be called without
/// file I/O (e.g. from fuzz targets).
pub fn render_template_str(content: &str, secrets: &SecretsFile) -> Result<String> {
    // ── Phase 1: scan for referenced placeholder names ───────────────────
    // Single left-to-right pass over the template. Both `{{` and `}}` are
    // ASCII, so byte-level `str::find` is correct even with multi-byte UTF-8.
    let referenced: std::collections::HashSet<&str> = {
        let mut set = std::collections::HashSet::new();
        let mut rest = content;
        while let Some(open) = rest.find("{{") {
            let after_open = &rest[open + 2..];
            if let Some(close) = after_open.find("}}") {
                let name = after_open[..close].trim();
                if !name.is_empty() {
                    set.insert(name);
                }
                rest = &after_open[close + 2..];
            } else {
                break;
            }
        }
        set
    };

    // ── Phase 2: fetch only the secrets this template actually uses ──────
    let mut fetched: std::collections::HashMap<String, Zeroizing<String>> =
        std::collections::HashMap::new();
    let mut failed: Vec<String> = Vec::new();
    for (name, uri) in &secrets.secrets {
        if !referenced.contains(name.as_str()) {
            continue;
        }
        match fetch_secret(uri) {
            Ok(val) => {
                fetched.insert(name.clone(), val);
            }
            Err(e) => failed.push(format!("{name} ({uri}): {e}")),
        }
    }
    if !failed.is_empty() {
        anyhow::bail!(
            "Failed to fetch {} secret(s):\n  {}",
            failed.len(),
            failed.join("\n  ")
        );
    }

    // ── Phase 3: single-pass substitution ────────────────────────────────
    // Walk the template left-to-right, emitting literal text and replacing
    // {{NAME}} at the point of encounter. Substituted values are never
    // re-scanned, so a secret value containing `{{OTHER}}` is emitted
    // verbatim — no cross-secret injection or order-dependent corruption.
    let mut result = String::with_capacity(content.len());
    let mut rest = content;
    let mut missing: Vec<String> = Vec::new();
    let mut missing_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        if let Some(close) = after_open.find("}}") {
            let name = after_open[..close].trim();
            result.push_str(&rest[..open]); // literal text before {{
            if let Some(val) = fetched.get(name) {
                result.push_str(val.as_str());
            } else if name.is_empty() {
                // Empty placeholder `{{}}` — emit as literal
                result.push_str("{{}}");
            } else {
                if missing_set.insert(name.to_string()) {
                    missing.push(name.to_string());
                }
                // Emit the placeholder as-is so the error is visible in context
                result.push_str(&rest[open..open + 2 + close + 2]);
            }
            rest = &after_open[close + 2..]; // advance past }}
        } else {
            break; // unclosed {{ — emit remainder as literal
        }
    }
    result.push_str(rest);

    // Strict mode: report all unreplaced placeholders at once.
    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|n| format!("{{{{{n}}}}}"))
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("Template contains unreplaced placeholder(s): {list}");
    }

    // `fetched` drops here; each Zeroizing<String> zeroes on drop.
    Ok(result)
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

    // Refuse to write into a world-writable directory — an attacker could race
    // to read or swap the tempfile before rename completes.
    #[cfg(unix)]
    {
        let parent_mode = parent
            .metadata()
            .with_context(|| format!("Failed to stat {}", parent.display()))?
            .permissions()
            .mode();
        if parent_mode & 0o002 != 0 {
            anyhow::bail!(
                "Refusing to write to {}: parent directory {} is world-writable (mode {:o})",
                path.display(),
                parent.display(),
                parent_mode & 0o777
            );
        }
    }

    let tmp = tempfile::Builder::new()
        .tempfile_in(parent)
        .with_context(|| format!("Failed to create tempfile in {}", parent.display()))?;

    // Write data through the owned handle, then sync, then set permissions,
    // then rename. This ordering ensures:
    //   1. Data is fully written before permissions are relaxed.
    //   2. A partial write (disk full) leaves the tempfile with no data and
    //      restricted permissions — not a partially-written world-readable file.
    tmp.as_file()
        .write_all(data)
        .with_context(|| format!("Failed to write to tempfile for {}", path.display()))?;
    // Retry sync_all on EINTR (signal interruption during fsync).
    loop {
        match tmp.as_file().sync_all() {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(anyhow!(e).context("Failed to fsync tempfile")),
        }
    }
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))
        .context("Failed to set permissions on tempfile")?;

    tmp.persist(path).map_err(|e| {
        // EXDEV (errno 18) = cross-device rename; other errors get no extra hint.
        let hint = if e.error.raw_os_error() == Some(18) {
            " (tempfile and target are on different filesystems)"
        } else {
            ""
        };
        anyhow!(
            "Failed to rename tempfile to {}: {}{}",
            path.display(),
            e.error,
            hint
        )
    })?;

    // Best-effort: sync parent directory so the rename is durable on crash.
    if let Some(parent) = path.parent() {
        match std::fs::File::open(parent).and_then(|d| d.sync_all()) {
            Ok(()) => {}
            Err(e) => eprintln!(
                "warning: failed to sync directory {}: {e}",
                parent.display()
            ),
        }
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
    // Fast path: link already points to the right place.
    if link.symlink_metadata().is_ok() {
        match fs::read_link(link) {
            Ok(existing) if existing == target => return Ok(()),
            Ok(_) => { /* stale symlink — will be replaced atomically below */ }
            Err(_) => {
                // Not a symlink (regular file or unreadable) — refuse to clobber.
                anyhow::bail!(
                    "Refusing to replace non-symlink at {} — remove it manually if intended",
                    link.display()
                );
            }
        }
    }

    let parent = link
        .parent()
        .ok_or_else(|| anyhow!("Symlink path has no parent: {}", link.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create parent dir {}", parent.display()))?;

    // Clean up any orphaned temp symlinks from prior crashed runs.
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".dotf-link-")
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    // Atomic symlink replacement: create a temp symlink with a unique name in
    // the same directory, then rename over the target. We generate the name
    // directly (instead of using tempfile + drop) to avoid a TOCTOU window
    // where another process could claim the path between drop and symlink.
    let tmp_path = parent.join(format!(
        ".dotf-link-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            ^ std::process::id() as u128
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs as unix_fs;
        unix_fs::symlink(target, &tmp_path).with_context(|| {
            format!(
                "Failed to create temp symlink {} -> {}",
                tmp_path.display(),
                target.display()
            )
        })?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs as win_fs;
        win_fs::symlink_file(target, &tmp_path).with_context(|| {
            format!(
                "Failed to create temp symlink {} -> {}\n  \
                 On Windows, symlinks require Developer Mode enabled or elevated privileges.\n  \
                 Settings → Update & Security → For Developers → Developer Mode",
                tmp_path.display(),
                target.display()
            )
        })?;
    }

    fs::rename(&tmp_path, link).with_context(|| {
        // Clean up the temp symlink on rename failure.
        let _ = fs::remove_file(&tmp_path);
        format!(
            "Failed to rename temp symlink to {} -> {}",
            link.display(),
            target.display()
        )
    })
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
        let _g = crate::env_lock();
        let p = expand_tilde("~").unwrap();
        assert_eq!(p, dirs::home_dir().unwrap());
    }

    #[test]
    fn expand_tilde_tilde_slash() {
        let _g = crate::env_lock();
        let p = expand_tilde("~/.gitconfig").unwrap();
        assert_eq!(p, dirs::home_dir().unwrap().join(".gitconfig"));
    }

    #[test]
    fn expand_tilde_nested_path() {
        let _g = crate::env_lock();
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
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "email = {{EMAIL}}\ntoken = {{TOKEN}}").unwrap();

        let _email = crate::EnvGuard::set("_DOTF_T_EMAIL", "chris@example.com");
        let _token = crate::EnvGuard::set("_DOTF_T_TOKEN", "abc123");

        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("EMAIL".to_string(), "env://_DOTF_T_EMAIL".to_string()),
                ("TOKEN".to_string(), "env://_DOTF_T_TOKEN".to_string()),
            ]),
        };

        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "email = chris@example.com\ntoken = abc123");
    }

    #[test]
    fn render_template_unknown_placeholder_errors_in_strict_mode() {
        // strict mode: referencing a placeholder not in secrets is a hard error
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "name = {{MISSING}}").unwrap();

        let secrets = SecretsFile::default();
        let err = render_template(&tmpl, &secrets).unwrap_err();
        let chain = format!("{err:?}");
        assert!(chain.contains("MISSING"), "unexpected error: {err}");
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
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "key = {{VAL}}").unwrap();

        let _val = crate::EnvGuard::set("_DOTF_T_VAL", "line1\nline2");
        let secrets = SecretsFile {
            secrets: HashMap::from([("VAL".to_string(), "env://_DOTF_T_VAL".to_string())]),
        };

        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "key = line1\nline2");
    }

    #[test]
    fn render_template_partial_secret_failure_reports_all() {
        let _g = crate::env_lock();
        // One secret resolves, one doesn't — error should mention the failed one.
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "a = {{GOOD}}\nb = {{BAD}}").unwrap();

        let _partial = crate::EnvGuard::set("_DOTF_T_PARTIAL", "ok");
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("GOOD".to_string(), "env://_DOTF_T_PARTIAL".to_string()),
                ("BAD".to_string(), "env://_DOTF_NONEXISTENT_XYZ".to_string()),
            ]),
        };

        let err = render_template(&tmpl, &secrets).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("BAD"),
            "error should name the failed secret: {msg}"
        );
        assert!(msg.contains("1 secret(s)"));
    }

    #[test]
    fn render_template_skips_unreferenced_secrets() {
        let _g = crate::env_lock();
        // Secret not referenced in template should not be fetched at all.
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "key = {{USED}}").unwrap();

        let _used = crate::EnvGuard::set("_DOTF_T_USED", "value");
        // UNUSED references a missing env var — would fail if fetched.
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("USED".to_string(), "env://_DOTF_T_USED".to_string()),
                (
                    "UNUSED".to_string(),
                    "env://_DOTF_NONEXISTENT_ABC".to_string(),
                ),
            ]),
        };

        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "key = value");
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
        use std::os::unix::fs::MetadataExt;

        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        let link = tmp.path().join("link.conf");

        fs::write(&target, "contents").unwrap();
        ensure_symlink(&target, &link).unwrap();

        // Record the inode of the symlink itself.
        let ino_before = link.symlink_metadata().unwrap().ino();

        // Call again — should be a true noop (no recreate).
        ensure_symlink(&target, &link).unwrap();

        let ino_after = link.symlink_metadata().unwrap().ino();
        assert_eq!(ino_before, ino_after, "symlink should not be recreated");
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

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_refuses_regular_file() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        let link = tmp.path().join("existing.conf");

        fs::write(&target, "contents").unwrap();
        fs::write(&link, "regular file").unwrap(); // regular file, not a symlink

        let err = ensure_symlink(&target, &link).unwrap_err();
        assert!(err.to_string().contains("non-symlink"));
    }

    // ── render_and_write ─────────────────────────────────────────
    #[test]
    fn render_and_write_creates_output_file() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("cfg.tmpl");
        let out = tmp.path().join("cfg");

        let _host = crate::EnvGuard::set("_DOTF_T_HOST", "myhost");
        fs::write(&tmpl, "host = {{HOST}}").unwrap();

        let secrets = SecretsFile {
            secrets: HashMap::from([("HOST".to_string(), "env://_DOTF_T_HOST".to_string())]),
        };

        render_and_write(&tmpl, &out, &secrets).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "host = myhost");
    }

    // ── corrupted TOML ──────────────────────────────────────────
    #[test]
    fn read_secrets_corrupted_toml_errors() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let _home = crate::EnvGuard::set("HOME", tmp.path().to_str().unwrap());
        let dotfiles = tmp.path().join(".dotf");
        fs::create_dir_all(dotfiles.join("configs")).unwrap();
        fs::write(dotfiles.join(".secrets.toml"), "{{invalid toml!!!").unwrap();
        let ctx = DotfContext::global();
        let err = ctx.read_secrets().unwrap_err();
        assert!(
            err.to_string().contains("parse") || err.to_string().contains("TOML"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn read_symlinks_corrupted_toml_errors() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let _home = crate::EnvGuard::set("HOME", tmp.path().to_str().unwrap());
        let dotfiles = tmp.path().join(".dotf");
        fs::create_dir_all(dotfiles.join("configs")).unwrap();
        fs::write(dotfiles.join(".symlinks.toml"), "not valid toml [[[").unwrap();
        let ctx = DotfContext::global();
        let err = ctx.read_symlinks().unwrap_err();
        assert!(
            err.to_string().contains("parse") || err.to_string().contains("TOML"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn read_secrets_unknown_field_rejected() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let _home = crate::EnvGuard::set("HOME", tmp.path().to_str().unwrap());
        let dotfiles = tmp.path().join(".dotf");
        fs::create_dir_all(dotfiles.join("configs")).unwrap();
        fs::write(
            dotfiles.join(".secrets.toml"),
            "[secrets]\nFOO = \"env://FOO\"\n\n[extra]\nBAD = true\n",
        )
        .unwrap();
        let ctx = DotfContext::global();
        let err = ctx.read_secrets().unwrap_err();
        let chain = format!("{err:?}");
        assert!(
            chain.contains("unknown") || chain.contains("extra"),
            "deny_unknown_fields should reject: {chain}"
        );
    }

    // ── atomic_write permission denied ──────────────────────────
    #[cfg(unix)]
    #[test]
    fn atomic_write_to_readonly_dir_fails() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("readonly");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();

        let err = atomic_write(&dir.join("file.txt"), b"data", 0o644).unwrap_err();
        // Restore permissions so TempDir cleanup works.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            err.to_string().contains("tempfile") || err.to_string().contains("Permission"),
            "unexpected: {err}"
        );
    }

    // ── orphaned temp symlink cleanup ───────────────────────────
    #[cfg(unix)]
    #[test]
    fn ensure_symlink_cleans_orphaned_temps() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("rendered.conf");
        fs::write(&target, "contents").unwrap();

        // Simulate an orphaned temp symlink from a prior crashed run.
        let orphan = tmp.path().join(".dotf-link-deadbeef");
        std::os::unix::fs::symlink("/nonexistent", &orphan).unwrap();
        assert!(orphan.symlink_metadata().is_ok());

        let link = tmp.path().join("mylink.conf");
        ensure_symlink(&target, &link).unwrap();

        // Orphan should be cleaned up.
        assert!(
            orphan.symlink_metadata().is_err(),
            "orphaned temp symlink should have been removed"
        );
        // Actual link should work.
        assert_eq!(fs::read_link(&link).unwrap(), target);
    }

    // ── placeholder name validation at parse time ────────────────
    #[test]
    fn secrets_file_rejects_invalid_placeholder_names() {
        let toml_str = "[secrets]\n\"invalid-name\" = \"env://FOO\"\n";
        let err: std::result::Result<SecretsFile, _> = toml::from_str(toml_str);
        assert!(
            err.is_err(),
            "hyphen in placeholder name should be rejected"
        );
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("invalid-name") || msg.contains("Invalid placeholder"));
    }

    #[test]
    fn secrets_file_rejects_empty_placeholder_name() {
        let toml_str = "[secrets]\n\"\" = \"env://FOO\"\n";
        let err: std::result::Result<SecretsFile, _> = toml::from_str(toml_str);
        assert!(err.is_err(), "empty placeholder name should be rejected");
    }

    #[test]
    fn secrets_file_accepts_valid_placeholder_names() {
        let toml_str = "[secrets]\nFOO_BAR = \"env://FOO\"\nX123 = \"env://X\"\n";
        let result: std::result::Result<SecretsFile, _> = toml::from_str(toml_str);
        assert!(result.is_ok());
    }

    #[test]
    fn secrets_file_rejects_invalid_uri_scheme() {
        let toml_str = "[secrets]\nMY_KEY = \"https://example.com/secret\"\n";
        let result: std::result::Result<SecretsFile, _> = toml::from_str(toml_str);
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Invalid secret URI"),
            "should reject invalid URI scheme: {msg}"
        );
    }

    #[test]
    fn secrets_file_rejects_empty_uri() {
        let toml_str = "[secrets]\nMY_KEY = \"\"\n";
        let result: std::result::Result<SecretsFile, _> = toml::from_str(toml_str);
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Invalid secret URI"),
            "should reject empty URI: {msg}"
        );
    }

    // ── Template safety ─────────────────────────────────────────
    #[test]
    fn template_rejects_unknown_placeholders() {
        // Verify that unreplaced {{...}} blocks are caught — only defined
        // secret names get replaced; anything else is an error.
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "{{dangerous_helper 'arg'}}").unwrap();

        let secrets = SecretsFile::default();
        let err = render_template(&tmpl, &secrets).unwrap_err();
        let chain = format!("{err:?}");
        assert!(
            chain.contains("unreplaced placeholder"),
            "unknown placeholders should fail: {chain}"
        );
    }

    // ── placeholder scanning ────────────────────────────────────
    #[test]
    fn render_template_handles_adjacent_placeholders() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "{{A}}{{B}}").unwrap();

        let _a = crate::EnvGuard::set("_DOTF_T_ADJ_A", "hello");
        let _b = crate::EnvGuard::set("_DOTF_T_ADJ_B", "world");
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("A".to_string(), "env://_DOTF_T_ADJ_A".to_string()),
                ("B".to_string(), "env://_DOTF_T_ADJ_B".to_string()),
            ]),
        };
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "helloworld");
    }

    #[test]
    fn render_template_secret_value_containing_braces_not_reinterpreted() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        // Template has {{A}} and {{B}}. A's value contains "{{B}}" literally.
        // The single-pass renderer must NOT re-substitute B inside A's value.
        fs::write(&tmpl, "first={{A}} second={{B}}").unwrap();

        let _a = crate::EnvGuard::set("_DOTF_T_INJ_A", "value_with_{{B}}_inside");
        let _b = crate::EnvGuard::set("_DOTF_T_INJ_B", "real_b");
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("A".to_string(), "env://_DOTF_T_INJ_A".to_string()),
                ("B".to_string(), "env://_DOTF_T_INJ_B".to_string()),
            ]),
        };
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "first=value_with_{{B}}_inside second=real_b");
    }

    #[test]
    fn render_template_handles_unicode_around_placeholders() {
        let _g = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        // Multi-byte UTF-8 characters around placeholders
        fs::write(&tmpl, "emoji 👍 {{VAL}} done 🎉").unwrap();

        let _uni = crate::EnvGuard::set("_DOTF_T_UNI", "ok");
        let secrets = SecretsFile {
            secrets: HashMap::from([("VAL".to_string(), "env://_DOTF_T_UNI".to_string())]),
        };
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "emoji 👍 ok done 🎉");
    }

    #[test]
    fn render_template_handles_unclosed_braces() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        // Unclosed {{ should not panic or infinite loop
        fs::write(&tmpl, "prefix {{UNCLOSED no closing").unwrap();

        let secrets = SecretsFile::default();
        // No matching close }} so no placeholder detected — should not panic
        let result = render_template(&tmpl, &secrets);
        // We just care that it doesn't panic — error is acceptable
        let _ = result;
    }

    #[test]
    fn render_template_three_sequential_placeholders() {
        let _g = crate::env_lock();
        // Three adjacent placeholders — verifies scanner advances correctly
        // past each one. A broken search_from offset would miss the third.
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        fs::write(&tmpl, "{{A}}{{B}}{{C}}").unwrap();

        let _a = crate::EnvGuard::set("_DOTF_T_3A", "1");
        let _b = crate::EnvGuard::set("_DOTF_T_3B", "2");
        let _c = crate::EnvGuard::set("_DOTF_T_3C", "3");
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("A".to_string(), "env://_DOTF_T_3A".to_string()),
                ("B".to_string(), "env://_DOTF_T_3B".to_string()),
                ("C".to_string(), "env://_DOTF_T_3C".to_string()),
                // Poison: would fail if fetched
                (
                    "UNUSED".to_string(),
                    "env://_DOTF_NONEXISTENT_3".to_string(),
                ),
            ]),
        };
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "123");
    }

    #[test]
    fn render_template_placeholder_scanning_precise_offset() {
        let _g = crate::env_lock();
        // Verify that after scanning {{X}}, the scanner starts AFTER the closing }},
        // not before it. Template: "{{X}}{{Y}}" — if scanner retreats, Y might be missed.
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("test.tmpl");
        // Y is required — if scanner misses it, strict mode will error
        fs::write(&tmpl, "{{X}}{{Y}}").unwrap();

        let _ox = crate::EnvGuard::set("_DOTF_T_OX", "a");
        let _oy = crate::EnvGuard::set("_DOTF_T_OY", "b");
        let secrets = SecretsFile {
            secrets: HashMap::from([
                ("X".to_string(), "env://_DOTF_T_OX".to_string()),
                ("Y".to_string(), "env://_DOTF_T_OY".to_string()),
            ]),
        };
        let rendered = render_template(&tmpl, &secrets).unwrap();
        assert_eq!(rendered, "ab");
    }

    // ── world-writable directory check ─────────────────────────
    #[cfg(unix)]
    #[test]
    fn atomic_write_rejects_world_writable_parent() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("world_writable");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();

        let err = atomic_write(&dir.join("file.txt"), b"data", 0o600).unwrap_err();
        // Restore permissions for cleanup.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            err.to_string().contains("world-writable"),
            "should reject world-writable dir: {err}"
        );
    }

    // ── DotfContext ─────────────────────────────────────────────

    #[test]
    fn ctx_global_dotfiles_dir() {
        let _g = crate::env_lock();
        let ctx = DotfContext::global();
        let dir = ctx.dotfiles_dir().unwrap();
        assert!(dir.ends_with(".dotf"));
    }

    #[test]
    fn ctx_local_dotfiles_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        let dir = ctx.dotfiles_dir().unwrap();
        assert_eq!(dir, tmp.path().join(".dotf"));
    }

    #[test]
    fn ctx_local_configs_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        assert_eq!(ctx.configs_dir().unwrap(), tmp.path().join(".dotf/configs"));
    }

    #[test]
    fn ctx_local_resolve_symlink_target_relative() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        let resolved = ctx.resolve_symlink_target(".env").unwrap();
        assert_eq!(resolved, tmp.path().join(".env"));
    }

    #[test]
    fn ctx_local_resolve_symlink_target_rejects_absolute() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        let err = ctx.resolve_symlink_target("/etc/hosts").unwrap_err();
        assert!(err.to_string().contains("relative paths"));
    }

    #[test]
    fn ctx_local_resolve_symlink_target_rejects_tilde() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        let err = ctx.resolve_symlink_target("~/.gitconfig").unwrap_err();
        assert!(err.to_string().contains("relative paths"));
    }

    #[test]
    fn ctx_local_root_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        assert_eq!(ctx.root_dir().unwrap(), tmp.path().to_path_buf());
    }

    #[test]
    fn ctx_local_read_write_secrets_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        fs::create_dir_all(ctx.dotfiles_dir().unwrap()).unwrap();

        let mut sf = SecretsFile::default();
        sf.secrets
            .insert("KEY".to_string(), "env://KEY".to_string());
        ctx.write_secrets(&sf).unwrap();
        let loaded = ctx.read_secrets().unwrap();
        assert_eq!(loaded.secrets["KEY"], "env://KEY");
    }

    #[test]
    fn ctx_local_read_write_symlinks_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ctx = DotfContext::local(tmp.path().to_path_buf());
        fs::create_dir_all(ctx.dotfiles_dir().unwrap()).unwrap();

        let mut sl = SymlinksFile::default();
        sl.symlinks.insert(".env".to_string(), ".env".to_string());
        ctx.write_symlinks(&sl).unwrap();
        let loaded = ctx.read_symlinks().unwrap();
        assert_eq!(loaded.symlinks[".env"], ".env");
    }

    #[cfg(unix)]
    #[test]
    fn ctx_local_render_and_symlink_all() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let ctx = DotfContext::local(root.to_path_buf());

        let dotf_dir = root.join(".dotf");
        let configs = dotf_dir.join("configs");
        fs::create_dir_all(&configs).unwrap();

        // Template with no secrets
        fs::write(configs.join(".env.tmpl"), "DB_HOST=localhost\n").unwrap();

        // Symlinks TOML
        let sl = SymlinksFile {
            symlinks: HashMap::from([(".env".to_string(), ".env".to_string())]),
        };
        fs::write(
            dotf_dir.join(".symlinks.toml"),
            toml::to_string_pretty(&sl).unwrap(),
        )
        .unwrap();
        fs::write(dotf_dir.join(".secrets.toml"), "[secrets]\n").unwrap();

        let done = ctx.render_and_symlink_all().unwrap();
        assert_eq!(done.len(), 1);

        // Symlink should exist at project root
        let link = root.join(".env");
        assert!(link.symlink_metadata().is_ok());
        assert_eq!(fs::read_to_string(&link).unwrap(), "DB_HOST=localhost\n");
    }

    #[cfg(unix)]
    #[test]
    fn ctx_local_render_rejects_escape() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let ctx = DotfContext::local(root.to_path_buf());

        let dotf_dir = root.join(".dotf");
        let configs = dotf_dir.join("configs");
        fs::create_dir_all(&configs).unwrap();

        fs::write(configs.join("evil.tmpl"), "x").unwrap();

        // Target tries to escape via ../
        let sl = SymlinksFile {
            symlinks: HashMap::from([("evil".to_string(), "../escape".to_string())]),
        };
        fs::write(
            dotf_dir.join(".symlinks.toml"),
            toml::to_string_pretty(&sl).unwrap(),
        )
        .unwrap();
        fs::write(dotf_dir.join(".secrets.toml"), "[secrets]\n").unwrap();

        let err = ctx.render_and_symlink_all().unwrap_err();
        assert!(
            err.to_string().contains("outside"),
            "should reject path traversal: {err}"
        );
    }

    // ── find_dotf_root ────────────────────────────────────────────

    #[test]
    fn find_dotf_root_finds_at_cwd() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".dotf")).unwrap();
        assert_eq!(find_dotf_root(tmp.path()), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_dotf_root_finds_parent() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".dotf")).unwrap();
        let sub = tmp.path().join("a/b/c");
        fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_dotf_root(&sub), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_dotf_root_returns_none_when_absent() {
        let tmp = TempDir::new().unwrap();
        // No .dotf directory — should walk up and find nothing (tmpdir is deep enough)
        let sub = tmp.path().join("x/y");
        fs::create_dir_all(&sub).unwrap();
        // We can't guarantee it won't find one above tmp, but it shouldn't find one at sub
        let result = find_dotf_root(&sub);
        // The result should not be sub or sub's parents within tmp
        if let Some(ref root) = result {
            assert!(!root.starts_with(tmp.path()));
        }
    }

    #[test]
    fn find_dotf_root_prefers_closest() {
        let tmp = TempDir::new().unwrap();
        // .dotf at root
        fs::create_dir_all(tmp.path().join(".dotf")).unwrap();
        // .dotf at nested project
        let project = tmp.path().join("projects/myapp");
        fs::create_dir_all(project.join(".dotf")).unwrap();

        // From inside the nested project, find the closest .dotf
        let deep = project.join("src");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_dotf_root(&deep), Some(project.clone()));
    }

    #[cfg(unix)]
    #[test]
    fn is_owned_by_current_user_returns_true_for_own_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(is_owned_by_current_user(tmp.path()));
    }

    #[cfg(unix)]
    #[test]
    fn is_owned_by_current_user_returns_false_for_nonexistent() {
        assert!(!is_owned_by_current_user(Path::new(
            "/nonexistent/path/abc123"
        )));
    }

    // ── resolve_context_from ──────────────────────────────────────

    #[test]
    fn resolve_context_from_dotf_at_home_returns_global() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        fs::create_dir_all(home.join(".dotf")).unwrap();
        let cwd = home.join("some/project");
        fs::create_dir_all(&cwd).unwrap();

        let ctx = resolve_context_from(&cwd, &home);
        assert!(
            matches!(ctx.mode, DotfMode::Global),
            "expected Global when .dotf is at HOME, got {:?}",
            ctx.mode
        );
    }

    #[test]
    fn resolve_context_from_dotf_at_project_returns_local() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(home.join(".dotf")).unwrap();
        let project = tmp.path().join("home/dev/myproject");
        fs::create_dir_all(project.join(".dotf")).unwrap();
        let cwd = project.join("src");
        fs::create_dir_all(&cwd).unwrap();

        let ctx = resolve_context_from(&cwd, &home);
        assert!(
            matches!(&ctx.mode, DotfMode::Local(root) if *root == project.canonicalize().unwrap()),
            "expected Local(project), got {:?}",
            ctx.mode
        );
    }

    #[test]
    fn resolve_context_from_no_dotf_returns_global() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cwd = tmp.path().join("home/dev");
        fs::create_dir_all(&cwd).unwrap();

        let ctx = resolve_context_from(&cwd, &home);
        // No .dotf anywhere in tmp, but walk-up might find one above tmp.
        // At minimum, it should not be Local pointing inside tmp.
        if let DotfMode::Local(root) = &ctx.mode {
            assert!(
                !root.starts_with(tmp.path()),
                "should not detect local mode in tmp: {:?}",
                ctx.mode
            );
        }
    }

    #[test]
    fn resolve_context_from_prefers_project_over_home() {
        // Both home and a nested project have .dotf — the closer one (project) wins
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("fakehome");
        fs::create_dir_all(home.join(".dotf")).unwrap();
        let project = home.join("code/app");
        fs::create_dir_all(project.join(".dotf")).unwrap();

        let ctx = resolve_context_from(&project, &home);
        assert!(
            matches!(&ctx.mode, DotfMode::Local(_)),
            "expected Local when project .dotf is closer than home, got {:?}",
            ctx.mode
        );
    }
}
