/// Integration tests — exercise full render+symlink pipeline using temp directories.
/// These tests set HOME to a temp dir so dotfiles_dir() points somewhere safe.
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

use dotf::dotfiles::DotfContext;

// ── EnvGuard (local copy — integration tests are a separate crate) ──────────

struct EnvGuard {
    key: String,
    prev: Option<String>,
}

impl EnvGuard {
    /// Set an env var, returning a guard that restores the original value on drop.
    fn set(key: &str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, val);
        }
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe {
                std::env::set_var(&self.key, v);
            },
            None => unsafe {
                std::env::remove_var(&self.key);
            },
        }
    }
}

// ── serialisation helpers ─────────────────────────────────────────────────────
// All global-mode integration tests mutate the HOME env var, so they must not
// run concurrently.  Holding ENV_LOCK for the lifetime of each TestEnv ensures
// serialisation without requiring an external crate.

static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── global-mode helpers ─────────────────────────────────────────────────────

struct TestEnv {
    home_path: PathBuf,
    configs: PathBuf,
    ctx: DotfContext,
    // Drop order: _home_guard restores HOME, then _home deletes tmpdir, then _lock releases mutex.
    _home_guard: EnvGuard,
    _home: TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl TestEnv {
    fn new() -> Self {
        // Acquire the lock before touching HOME.
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let home = TempDir::new().unwrap();
        let home_path = home.path().to_path_buf();
        let dotfiles = home_path.join("dotfiles");
        let configs = dotfiles.join("configs");
        fs::create_dir_all(&configs).unwrap();

        // Redirect HOME so dirs::home_dir() / dotfiles_dir() point here.
        // EnvGuard saves the original HOME and restores it on drop.
        let home_guard = EnvGuard::set("HOME", &home_path.to_string_lossy());

        let ctx = DotfContext::global();

        TestEnv {
            _lock: lock,
            _home_guard: home_guard,
            _home: home,
            home_path,
            configs,
            ctx,
        }
    }

    fn write_template(&self, name: &str, content: &str) {
        fs::write(self.configs.join(format!("{name}.tmpl")), content).unwrap();
    }

    fn write_secrets_toml(&self, pairs: &[(&str, &str)]) {
        let dotfiles = self.home_path.join("dotfiles");
        let mut toml = String::from("[secrets]\n");
        for (k, v) in pairs {
            toml.push_str(&format!("\"{k}\" = \"{v}\"\n"));
        }
        fs::write(dotfiles.join(".secrets.toml"), toml).unwrap();
    }

    fn write_symlinks_toml(&self, pairs: &[(&str, &str)]) {
        let dotfiles = self.home_path.join("dotfiles");
        let mut toml = String::from("[symlinks]\n");
        for (k, v) in pairs {
            toml.push_str(&format!("\"{k}\" = \"{v}\"\n"));
        }
        fs::write(dotfiles.join(".symlinks.toml"), toml).unwrap();
    }

    fn rendered(&self, name: &str) -> String {
        fs::read_to_string(self.configs.join(name)).unwrap()
    }
}

// ── render pipeline ───────────────────────────────────────────────────────────

#[test]
fn render_and_symlink_all_basic() {
    let env = TestEnv::new();

    let _email = EnvGuard::set("_IT_EMAIL", "chris@example.com");
    let _token = EnvGuard::set("_IT_TOKEN", "tok123");

    env.write_template(
        ".gitconfig",
        "[user]\n  email = {{EMAIL}}\n  token = {{TOKEN}}",
    );
    env.write_secrets_toml(&[("EMAIL", "env://_IT_EMAIL"), ("TOKEN", "env://_IT_TOKEN")]);
    // symlink target inside home so we don't need to create ~/.gitconfig for real
    let link_target = format!("{}/.gitconfig", env.home_path.display());
    env.write_symlinks_toml(&[(".gitconfig", &link_target)]);

    // Run the full pipeline
    let done = env.ctx.render_and_symlink_all().unwrap();

    assert_eq!(done.len(), 1);
    assert_eq!(
        env.rendered(".gitconfig"),
        "[user]\n  email = chris@example.com\n  token = tok123"
    );

    // Symlink should exist and point to rendered file
    let link = PathBuf::from(&link_target);
    assert!(link.symlink_metadata().is_ok());
}

#[test]
fn render_and_symlink_all_missing_template_is_skipped() {
    let env = TestEnv::new();

    // Write symlinks.toml referencing a template that doesn't exist
    let link_target = format!("{}/.zshrc", env.home_path.display());
    env.write_symlinks_toml(&[(".zshrc", &link_target)]);
    env.write_secrets_toml(&[]);

    // Should not error — just skip
    let done = env.ctx.render_and_symlink_all().unwrap();
    assert!(done.is_empty());
}

#[test]
fn render_and_symlink_all_empty_secrets_toml() {
    let env = TestEnv::new();

    env.write_template(".zshrc", "# no secrets here\nexport EDITOR=nvim\n");
    env.write_secrets_toml(&[]);
    let link_target = format!("{}/.zshrc", env.home_path.display());
    env.write_symlinks_toml(&[(".zshrc", &link_target)]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(
        env.rendered(".zshrc"),
        "# no secrets here\nexport EDITOR=nvim\n"
    );
}

#[test]
fn render_and_symlink_all_multiple_configs() {
    let env = TestEnv::new();

    let _a = EnvGuard::set("_IT_MULTI_A", "valueA");
    let _b = EnvGuard::set("_IT_MULTI_B", "valueB");

    env.write_template("a.conf", "a = {{AA}}");
    env.write_template("b.conf", "b = {{BB}}");
    env.write_secrets_toml(&[("AA", "env://_IT_MULTI_A"), ("BB", "env://_IT_MULTI_B")]);

    let link_a = format!("{}/a.conf", env.home_path.display());
    let link_b = format!("{}/b.conf", env.home_path.display());
    env.write_symlinks_toml(&[("a.conf", &link_a), ("b.conf", &link_b)]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 2);

    assert_eq!(env.rendered("a.conf"), "a = valueA");
    assert_eq!(env.rendered("b.conf"), "b = valueB");
}

#[test]
fn render_and_symlink_all_re_render_updates_file() {
    let env = TestEnv::new();

    let _val = EnvGuard::set("_IT_UPDATE_VAL", "first");
    env.write_template("cfg", "val = {{VAL}}");
    env.write_secrets_toml(&[("VAL", "env://_IT_UPDATE_VAL")]);
    let link = format!("{}/cfg", env.home_path.display());
    env.write_symlinks_toml(&[("cfg", &link)]);

    env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(env.rendered("cfg"), "val = first");

    // Update the env var value and re-render — guard still alive, just overwrite
    unsafe {
        std::env::set_var("_IT_UPDATE_VAL", "second");
    }
    env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(env.rendered("cfg"), "val = second");
}

// ── secrets / symlinks toml round-trip ───────────────────────────────────────

#[test]
fn read_write_secrets_roundtrip() {
    let env = TestEnv::new();

    let mut sf = dotf::dotfiles::SecretsFile::default();
    sf.secrets
        .insert("FOO".to_string(), "env://FOO".to_string());
    sf.secrets
        .insert("BAR".to_string(), "op://vault/item/field".to_string());

    env.ctx.write_secrets(&sf).unwrap();
    let loaded = env.ctx.read_secrets().unwrap();

    assert_eq!(loaded.secrets["FOO"], "env://FOO");
    assert_eq!(loaded.secrets["BAR"], "op://vault/item/field");
}

#[test]
fn read_write_symlinks_roundtrip() {
    let env = TestEnv::new();

    let mut sl = dotf::dotfiles::SymlinksFile::default();
    sl.symlinks
        .insert(".gitconfig".to_string(), "~/.gitconfig".to_string());

    env.ctx.write_symlinks(&sl).unwrap();
    let loaded = env.ctx.read_symlinks().unwrap();

    assert_eq!(loaded.symlinks[".gitconfig"], "~/.gitconfig");
}

#[test]
fn render_and_symlink_all_rejects_outside_home() {
    let env = TestEnv::new();

    env.write_template("evil.conf", "no secrets");
    env.write_secrets_toml(&[]);
    // Target escapes HOME via ../..
    env.write_symlinks_toml(&[("evil.conf", "/tmp/dotf-escape-test")]);

    let err = env.ctx.render_and_symlink_all().unwrap_err();
    assert!(
        err.to_string().contains("outside"),
        "expected path traversal rejection, got: {err}"
    );
}

#[test]
fn read_secrets_returns_default_when_missing() {
    let env = TestEnv::new();
    // No .secrets.toml written
    let sf = env.ctx.read_secrets().unwrap();
    assert!(sf.secrets.is_empty());
}

#[test]
fn read_symlinks_returns_default_when_missing() {
    let env = TestEnv::new();
    let sl = env.ctx.read_symlinks().unwrap();
    assert!(sl.symlinks.is_empty());
}

// ── local mode tests ─────────────────────────────────────────────────────────

struct TestLocalEnv {
    _lock: MutexGuard<'static, ()>,
    _tmp: TempDir,
    root: PathBuf,
    configs: PathBuf,
    ctx: DotfContext,
}

impl TestLocalEnv {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let dotf_dir = root.join(".dotf");
        let configs = dotf_dir.join("configs");
        fs::create_dir_all(&configs).unwrap();

        let ctx = DotfContext::local(root.clone());

        TestLocalEnv {
            _lock: lock,
            _tmp: tmp,
            root,
            configs,
            ctx,
        }
    }

    fn write_template(&self, name: &str, content: &str) {
        fs::write(self.configs.join(format!("{name}.tmpl")), content).unwrap();
    }

    fn write_secrets_toml(&self, pairs: &[(&str, &str)]) {
        let mut toml = String::from("[secrets]\n");
        for (k, v) in pairs {
            toml.push_str(&format!("\"{k}\" = \"{v}\"\n"));
        }
        fs::write(self.root.join(".dotf/.secrets.toml"), toml).unwrap();
    }

    fn write_symlinks_toml(&self, pairs: &[(&str, &str)]) {
        let mut toml = String::from("[symlinks]\n");
        for (k, v) in pairs {
            toml.push_str(&format!("\"{k}\" = \"{v}\"\n"));
        }
        fs::write(self.root.join(".dotf/.symlinks.toml"), toml).unwrap();
    }

    fn rendered(&self, name: &str) -> String {
        fs::read_to_string(self.configs.join(name)).unwrap()
    }
}

#[test]
fn local_render_and_symlink_basic() {
    let env = TestLocalEnv::new();

    let _val = EnvGuard::set("_IT_LOCAL_VAL", "secret123");

    env.write_template(".env", "API_KEY={{API_KEY}}\n");
    env.write_secrets_toml(&[("API_KEY", "env://_IT_LOCAL_VAL")]);
    env.write_symlinks_toml(&[(".env", ".env")]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(env.rendered(".env"), "API_KEY=secret123\n");

    // Symlink at project root
    let link = env.root.join(".env");
    assert!(link.symlink_metadata().is_ok());
    assert_eq!(fs::read_to_string(&link).unwrap(), "API_KEY=secret123\n");
}

#[test]
fn local_render_and_symlink_no_secrets() {
    let env = TestLocalEnv::new();

    env.write_template(".env", "DB_HOST=localhost\n");
    env.write_secrets_toml(&[]);
    env.write_symlinks_toml(&[(".env", ".env")]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(env.rendered(".env"), "DB_HOST=localhost\n");
}

#[test]
fn local_rejects_path_escape() {
    let env = TestLocalEnv::new();

    env.write_template("evil", "x");
    env.write_secrets_toml(&[]);
    env.write_symlinks_toml(&[("evil", "../escape")]);

    let err = env.ctx.render_and_symlink_all().unwrap_err();
    assert!(
        err.to_string().contains("outside"),
        "should reject path traversal: {err}"
    );
}

#[test]
fn local_rejects_absolute_symlink_target() {
    let env = TestLocalEnv::new();

    env.write_template("cfg", "x");
    env.write_secrets_toml(&[]);
    env.write_symlinks_toml(&[("cfg", "/tmp/escape")]);

    let err = env.ctx.render_and_symlink_all().unwrap_err();
    assert!(
        err.to_string().contains("relative paths"),
        "should reject absolute target: {err}"
    );
}

#[test]
fn local_read_write_roundtrip() {
    let env = TestLocalEnv::new();

    let mut sf = dotf::dotfiles::SecretsFile::default();
    sf.secrets
        .insert("KEY".to_string(), "env://KEY".to_string());
    env.ctx.write_secrets(&sf).unwrap();
    let loaded = env.ctx.read_secrets().unwrap();
    assert_eq!(loaded.secrets["KEY"], "env://KEY");

    let mut sl = dotf::dotfiles::SymlinksFile::default();
    sl.symlinks.insert(".env".to_string(), ".env".to_string());
    env.ctx.write_symlinks(&sl).unwrap();
    let loaded = env.ctx.read_symlinks().unwrap();
    assert_eq!(loaded.symlinks[".env"], ".env");
}

#[test]
fn local_multiple_configs() {
    let env = TestLocalEnv::new();

    let _a = EnvGuard::set("_IT_LOCAL_A", "aaa");
    let _b = EnvGuard::set("_IT_LOCAL_B", "bbb");

    env.write_template(".env", "A={{A}}\n");
    env.write_template("settings.json", "{\"key\": \"{{B}}\"}\n");
    env.write_secrets_toml(&[("A", "env://_IT_LOCAL_A"), ("B", "env://_IT_LOCAL_B")]);
    env.write_symlinks_toml(&[(".env", ".env"), ("settings.json", "settings.json")]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 2);

    assert_eq!(env.rendered(".env"), "A=aaa\n");
    assert_eq!(env.rendered("settings.json"), "{\"key\": \"bbb\"}\n");
}

#[test]
fn local_subdirectory_symlink() {
    let env = TestLocalEnv::new();

    // Create subdirectory for target
    fs::create_dir_all(env.root.join("sub")).unwrap();

    env.write_template("cfg", "content\n");
    env.write_secrets_toml(&[]);
    env.write_symlinks_toml(&[("cfg", "sub/cfg")]);

    let done = env.ctx.render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 1);

    let link = env.root.join("sub/cfg");
    assert!(link.symlink_metadata().is_ok());
    assert_eq!(fs::read_to_string(&link).unwrap(), "content\n");
}
