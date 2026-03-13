/// Integration tests — exercise full render+symlink pipeline using temp directories.
/// These tests set HOME to a temp dir so dotfiles_dir() points somewhere safe.
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

// ── serialisation helpers ─────────────────────────────────────────────────────
// All integration tests mutate the HOME env var, so they must not run
// concurrently.  Holding ENV_LOCK for the lifetime of each TestEnv ensures
// serialisation without requiring an external crate.

static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── helpers ──────────────────────────────────────────────────────────────────

struct TestEnv {
    _lock: MutexGuard<'static, ()>,
    _home: TempDir,
    home_path: PathBuf,
    dotfiles: PathBuf,
    configs: PathBuf,
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

        // Redirect HOME so dirs::home_dir() / dotfiles_dir() point here
        unsafe {
            std::env::set_var("HOME", &home_path);
        }

        TestEnv {
            _lock: lock,
            _home: home,
            home_path,
            dotfiles,
            configs,
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
        fs::write(self.dotfiles.join(".secrets.toml"), toml).unwrap();
    }

    fn write_symlinks_toml(&self, pairs: &[(&str, &str)]) {
        let mut toml = String::from("[symlinks]\n");
        for (k, v) in pairs {
            toml.push_str(&format!("\"{k}\" = \"{v}\"\n"));
        }
        fs::write(self.dotfiles.join(".symlinks.toml"), toml).unwrap();
    }

    fn rendered(&self, name: &str) -> String {
        fs::read_to_string(self.configs.join(name)).unwrap()
    }
}

// ── render pipeline ───────────────────────────────────────────────────────────

#[test]
fn render_and_symlink_all_basic() {
    let env = TestEnv::new();

    unsafe {
        std::env::set_var("_IT_EMAIL", "chris@example.com");
    }
    unsafe {
        std::env::set_var("_IT_TOKEN", "tok123");
    }

    env.write_template(
        ".gitconfig",
        "[user]\n  email = {{EMAIL}}\n  token = {{TOKEN}}",
    );
    env.write_secrets_toml(&[("EMAIL", "env://_IT_EMAIL"), ("TOKEN", "env://_IT_TOKEN")]);
    // symlink target inside home so we don't need to create ~/.gitconfig for real
    let link_target = format!("{}/.gitconfig", env.home_path.display());
    env.write_symlinks_toml(&[(".gitconfig", &link_target)]);

    // Run the full pipeline
    let done = dotf::dotfiles::render_and_symlink_all().unwrap();

    assert_eq!(done.len(), 1);
    assert_eq!(
        env.rendered(".gitconfig"),
        "[user]\n  email = chris@example.com\n  token = tok123"
    );

    // Symlink should exist and point to rendered file
    let link = PathBuf::from(&link_target);
    assert!(link.symlink_metadata().is_ok());

    unsafe {
        std::env::remove_var("_IT_EMAIL");
    }
    unsafe {
        std::env::remove_var("_IT_TOKEN");
    }
}

#[test]
fn render_and_symlink_all_missing_template_is_skipped() {
    let env = TestEnv::new();

    // Write symlinks.toml referencing a template that doesn't exist
    let link_target = format!("{}/.zshrc", env.home_path.display());
    env.write_symlinks_toml(&[(".zshrc", &link_target)]);
    env.write_secrets_toml(&[]);

    // Should not error — just skip
    let done = dotf::dotfiles::render_and_symlink_all().unwrap();
    assert!(done.is_empty());
}

#[test]
fn render_and_symlink_all_empty_secrets_toml() {
    let env = TestEnv::new();

    env.write_template(".zshrc", "# no secrets here\nexport EDITOR=nvim\n");
    env.write_secrets_toml(&[]);
    let link_target = format!("{}/.zshrc", env.home_path.display());
    env.write_symlinks_toml(&[(".zshrc", &link_target)]);

    let done = dotf::dotfiles::render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(
        env.rendered(".zshrc"),
        "# no secrets here\nexport EDITOR=nvim\n"
    );
}

#[test]
fn render_and_symlink_all_multiple_configs() {
    let env = TestEnv::new();

    unsafe {
        std::env::set_var("_IT_MULTI_A", "valueA");
    }
    unsafe {
        std::env::set_var("_IT_MULTI_B", "valueB");
    }

    env.write_template("a.conf", "a = {{AA}}");
    env.write_template("b.conf", "b = {{BB}}");
    env.write_secrets_toml(&[("AA", "env://_IT_MULTI_A"), ("BB", "env://_IT_MULTI_B")]);

    let link_a = format!("{}/a.conf", env.home_path.display());
    let link_b = format!("{}/b.conf", env.home_path.display());
    env.write_symlinks_toml(&[("a.conf", &link_a), ("b.conf", &link_b)]);

    let done = dotf::dotfiles::render_and_symlink_all().unwrap();
    assert_eq!(done.len(), 2);

    assert_eq!(env.rendered("a.conf"), "a = valueA");
    assert_eq!(env.rendered("b.conf"), "b = valueB");

    unsafe {
        std::env::remove_var("_IT_MULTI_A");
    }
    unsafe {
        std::env::remove_var("_IT_MULTI_B");
    }
}

#[test]
fn render_and_symlink_all_re_render_updates_file() {
    let env = TestEnv::new();

    unsafe {
        std::env::set_var("_IT_UPDATE_VAL", "first");
    }
    env.write_template("cfg", "val = {{VAL}}");
    env.write_secrets_toml(&[("VAL", "env://_IT_UPDATE_VAL")]);
    let link = format!("{}/cfg", env.home_path.display());
    env.write_symlinks_toml(&[("cfg", &link)]);

    dotf::dotfiles::render_and_symlink_all().unwrap();
    assert_eq!(env.rendered("cfg"), "val = first");

    unsafe {
        std::env::set_var("_IT_UPDATE_VAL", "second");
    }
    dotf::dotfiles::render_and_symlink_all().unwrap();
    assert_eq!(env.rendered("cfg"), "val = second");

    unsafe {
        std::env::remove_var("_IT_UPDATE_VAL");
    }
}

// ── secrets / symlinks toml round-trip ───────────────────────────────────────

#[test]
fn read_write_secrets_roundtrip() {
    let env = TestEnv::new();
    let _ = &env; // ensure HOME is set and lock is held

    let mut sf = dotf::dotfiles::SecretsFile::default();
    sf.secrets
        .insert("FOO".to_string(), "env://FOO".to_string());
    sf.secrets
        .insert("BAR".to_string(), "op://vault/item/field".to_string());

    dotf::dotfiles::write_secrets(&sf).unwrap();
    let loaded = dotf::dotfiles::read_secrets().unwrap();

    assert_eq!(loaded.secrets["FOO"], "env://FOO");
    assert_eq!(loaded.secrets["BAR"], "op://vault/item/field");
}

#[test]
fn read_write_symlinks_roundtrip() {
    let env = TestEnv::new();
    let _ = &env;

    let mut sl = dotf::dotfiles::SymlinksFile::default();
    sl.symlinks
        .insert(".gitconfig".to_string(), "~/.gitconfig".to_string());

    dotf::dotfiles::write_symlinks(&sl).unwrap();
    let loaded = dotf::dotfiles::read_symlinks().unwrap();

    assert_eq!(loaded.symlinks[".gitconfig"], "~/.gitconfig");
}

#[test]
fn render_and_symlink_all_rejects_outside_home() {
    let env = TestEnv::new();

    env.write_template("evil.conf", "no secrets");
    env.write_secrets_toml(&[]);
    // Target escapes HOME via ../..
    env.write_symlinks_toml(&[("evil.conf", "/tmp/dotf-escape-test")]);

    let err = dotf::dotfiles::render_and_symlink_all().unwrap_err();
    assert!(
        err.to_string().contains("outside home directory"),
        "expected path traversal rejection, got: {err}"
    );
}

#[test]
fn read_secrets_returns_default_when_missing() {
    let env = TestEnv::new();
    let _ = &env;
    // No .secrets.toml written
    let sf = dotf::dotfiles::read_secrets().unwrap();
    assert!(sf.secrets.is_empty());
}

#[test]
fn read_symlinks_returns_default_when_missing() {
    let env = TestEnv::new();
    let _ = &env;
    let sl = dotf::dotfiles::read_symlinks().unwrap();
    assert!(sl.symlinks.is_empty());
}
