use anyhow::{Context, Result, anyhow};
use dialoguer::{Input, theme::ColorfulTheme};
use std::fs;
use which::which;

use crate::dotfiles;
use crate::dotfiles::{DotfContext, DotfMode};
use crate::runner::Runner;
use crate::ui::UI;

pub fn run(ui: &UI, runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    if matches!(&ctx.mode, DotfMode::Local(_)) {
        return run_local(ui, ctx);
    }

    ui.header();

    setup_dotfiles_dir(ui, runner, ctx)?;
    hint_secret_backends(ui, ctx)?;
    #[cfg(target_os = "macos")]
    run_brewfile(ui, runner, ctx)?;
    #[cfg(unix)]
    install_completions(ui, runner)?;

    let synced = ctx.render_and_symlink_all()?;

    ui.blank();
    if synced.is_empty() {
        ui.finished(format!(
            "setup complete — run {} to add a config",
            ui.highlight("dotf config <path>")
        ));
    } else {
        for entry in &synced {
            ui.action("Linked", entry);
        }
        ui.finished(format!("setup complete — {} configs linked", synced.len()));
    }

    Ok(())
}

fn run_local(ui: &UI, ctx: &DotfContext) -> Result<()> {
    let dotf_dir = ctx.dotfiles_dir()?;
    let configs_dir = ctx.configs_dir()?;

    // Warn if a parent directory already has .dotf/ — creating a nested one
    // is almost certainly a mistake.
    if let DotfMode::Local(root) = &ctx.mode
        && let Some(parent) = root.parent()
        && let Some(existing) = dotfiles::find_dotf_root(parent)
    {
        ui.warn(
            "Warning",
            format!("A .dotf/ directory already exists at {}", existing.display()),
        );
        ui.hint(format!(
            "Creating a nested .dotf/ at {} — is this intentional?",
            root.display()
        ));
        ui.blank();
    }

    fs::create_dir_all(&configs_dir)
        .with_context(|| format!("Failed to create {}", configs_dir.display()))?;
    ui.action("Creating", format!("{}", dotf_dir.display()));

    // Append .gitignore entries (idempotent)
    let root = ctx.root_dir()?;
    let gitignore_path = root.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();

    let entries = [
        ".dotf/configs/*",
        "!.dotf/configs/*.tmpl",
        ".dotf/.secrets.toml",
    ];
    let mut to_add = Vec::new();
    for entry in &entries {
        if !existing.lines().any(|line| line.trim() == *entry) {
            to_add.push(*entry);
        }
    }

    if !to_add.is_empty() {
        let mut content = existing;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        for entry in &to_add {
            content.push_str(entry);
            content.push('\n');
        }
        fs::write(&gitignore_path, &content)
            .with_context(|| format!("Failed to write {}", gitignore_path.display()))?;
        ui.action(
            "Updated",
            format!(".gitignore ({} entries added)", to_add.len()),
        );
    } else {
        ui.skip("Skipped", ".gitignore already configured");
    }

    let synced = ctx.render_and_symlink_all()?;

    ui.blank();
    if synced.is_empty() {
        ui.finished(format!(
            "local setup complete — run {} to add a config",
            ui.highlight("dotf config <path>")
        ));
    } else {
        for entry in &synced {
            ui.action("Linked", entry);
        }
        ui.finished(format!(
            "local setup complete — {} configs linked",
            synced.len()
        ));
    }

    Ok(())
}

fn setup_dotfiles_dir(ui: &UI, runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    let dotfiles = ctx.dotfiles_dir()?;

    if dotfiles.exists() {
        ui.skip("Skipped", format!("{} already exists", dotfiles.display()));
        return Ok(());
    }

    // Migration hint: if old ~/dotfiles exists, tell the user.
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_dotfiles = home.join("dotfiles");
    if old_dotfiles.exists() {
        ui.warn("Found", "existing ~/dotfiles — dotf now uses ~/.dotf/");
        ui.hint("Run: mv ~/dotfiles ~/.dotf");
        ui.blank();
    }

    ui.warn("Missing", "~/.dotf not found");

    let url: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Git repo URL to clone (leave blank to create fresh repo)")
        .allow_empty(true)
        .interact_text()
        .context("Failed to read git repo URL")?;

    if url.is_empty() {
        fs::create_dir_all(&dotfiles)
            .with_context(|| format!("Failed to create {}", dotfiles.display()))?;

        let init = runner.run("git", &["init", "-b", "main"], Some(&dotfiles))?;
        if !init.success() {
            let init2 = runner.run("git", &["init"], Some(&dotfiles))?;
            if !init2.success() {
                anyhow::bail!("git init failed");
            }
        }

        fs::create_dir_all(dotfiles.join("configs")).context("Failed to create configs dir")?;

        ui.action(
            "Creating",
            format!("fresh dotfiles repo at {}", dotfiles.display()),
        );

        if which("gh").is_ok() {
            let source = dotfiles
                .to_str()
                .ok_or_else(|| anyhow!("dotfiles path is not valid UTF-8"))?;
            let sp = ui.spinner("Creating GitHub repo…");
            let gh = runner.run(
                "gh",
                &[
                    "repo",
                    "create",
                    "dotfiles",
                    "--private",
                    &format!("--source={source}"),
                    "--remote=origin",
                ],
                None,
            )?;
            if gh.success() {
                sp.finish("Created", "GitHub repo created and remote set");
            } else {
                sp.finish_warn(
                    "Warning",
                    "gh repo create failed — you can set up the remote manually",
                );
            }
        } else {
            ui.skip("Skipped", "gh not found — skipping GitHub repo creation");
        }
    } else {
        let dotfiles_str = dotfiles
            .to_str()
            .ok_or_else(|| anyhow!("dotfiles path is not valid UTF-8"))?;
        let sp = ui.spinner(format!("Cloning {url}…"));
        let clone = runner.run("git", &["clone", &url, dotfiles_str], None)?;
        if !clone.success() {
            anyhow::bail!("git clone failed");
        }
        sp.finish("Cloned", format!("dotfiles to {}", dotfiles.display()));
    }

    Ok(())
}

fn hint_secret_backends(ui: &UI, ctx: &DotfContext) -> Result<()> {
    let secrets = ctx.read_secrets().unwrap_or_default();
    let mut needs_pass = false;
    let mut needs_op = false;
    let mut needs_bw = false;

    for uri in secrets.secrets.values() {
        if uri.starts_with("pass://") {
            needs_pass = true;
        } else if uri.starts_with("op://") {
            needs_op = true;
        } else if uri.starts_with("bw://") {
            needs_bw = true;
        }
    }

    if !needs_pass && !needs_op && !needs_bw {
        ui.skip(
            "Skipped",
            format!(
                "no secret backends configured — add secrets with {}",
                ui.highlight("dotf secrets add")
            ),
        );
        return Ok(());
    }

    if needs_pass {
        #[cfg(target_os = "macos")]
        check_cli(
            ui,
            "pass",
            "Proton Pass",
            "brew install protonpass/pass/pass",
        )?;
        #[cfg(not(target_os = "macos"))]
        check_cli(ui, "pass", "Proton Pass", "https://proton.me/pass/download")?;
    }
    if needs_op {
        #[cfg(target_os = "macos")]
        check_cli(ui, "op", "1Password", "brew install 1password-cli")?;
        #[cfg(not(target_os = "macos"))]
        check_cli(
            ui,
            "op",
            "1Password",
            "https://developer.1password.com/docs/cli/get-started/",
        )?;
    }
    if needs_bw {
        #[cfg(target_os = "macos")]
        check_cli(ui, "bw", "Bitwarden", "brew install bitwarden-cli")?;
        #[cfg(not(target_os = "macos"))]
        check_cli(ui, "bw", "Bitwarden", "npm install -g @bitwarden/cli")?;
    }

    Ok(())
}

fn check_cli(ui: &UI, bin: &str, name: &str, install_hint: &str) -> Result<()> {
    if which(bin).is_ok() {
        ui.action("Detected", format!("{name} CLI ({bin})"));
    } else {
        ui.warn(
            "Missing",
            format!(
                "{name} CLI ({bin}) not found — install with: {}",
                ui.highlight(install_hint)
            ),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_brewfile(ui: &UI, runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    let brewfile = ctx.dotfiles_dir()?.join("Brewfile");
    if !brewfile.exists() {
        ui.skip("Skipped", "no Brewfile found");
        return Ok(());
    }

    let brewfile_str = brewfile
        .to_str()
        .ok_or_else(|| anyhow!("Brewfile path is not valid UTF-8"))?;
    let sp = ui.spinner("Running brew bundle…");
    let result = runner.run(
        "brew",
        &["bundle", "install", &format!("--file={brewfile_str}")],
        None,
    )?;
    if !result.success() {
        anyhow::bail!("brew bundle install failed");
    }
    sp.finish("Installed", "brew bundle complete");
    Ok(())
}

#[cfg(unix)]
fn install_completions(ui: &UI, runner: &dyn Runner) -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let completions_dir = home.join(".zfunc");
    fs::create_dir_all(&completions_dir).context("Failed to create ~/.zfunc")?;

    let completion_file = completions_dir.join("_dotf");
    // Use current_exe() so this works even before dotf is on PATH (e.g. first brew install).
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("dotf"));
    let exe_str = exe.to_string_lossy().into_owned();
    let result = runner.run(&exe_str, &["completions", "zsh"], None);

    match result {
        Ok(out) if out.success() => {
            dotfiles::atomic_write(&completion_file, out.stdout.as_bytes(), 0o644)
                .context("Failed to write zsh completion file")?;
            ui.action(
                "Installed",
                format!("zsh completions to {}", completion_file.display()),
            );
            ui.skip(
                "",
                "add to ~/.zshrc if not present: fpath=(~/.zfunc $fpath) && autoload -U compinit && compinit",
            );
        }
        _ => {
            ui.skip(
                "Skipped",
                "completions (dotf not in PATH yet — run after brew install)",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MockRunner;
    use tempfile::TempDir;

    struct InitEnv {
        // Drop order: _home_guard restores HOME, _tmp deletes dir, _lock releases mutex.
        _home_guard: crate::EnvGuard,
        _tmp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    fn init_env_with_dotfiles() -> InitEnv {
        let lock = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let dotfiles = tmp.path().join(".dotf");
        std::fs::create_dir_all(dotfiles.join("configs")).unwrap();
        std::fs::write(dotfiles.join(".symlinks.toml"), "[symlinks]\n").unwrap();
        std::fs::write(dotfiles.join(".secrets.toml"), "[secrets]\n").unwrap();
        let home_guard = crate::EnvGuard::set("HOME", &tmp.path().to_string_lossy());
        InitEnv {
            _home_guard: home_guard,
            _tmp: tmp,
            _lock: lock,
        }
    }

    #[test]
    fn init_dotfiles_already_exists_succeeds() {
        let _env = init_env_with_dotfiles();
        let runner = MockRunner::new().allow_unmatched();
        let ctx = DotfContext::global();
        run(&UI::new(), &runner, &ctx).unwrap();
    }

    #[test]
    fn run_local_creates_dotf_and_gitignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let ctx = DotfContext::local(root.clone());

        run_local(&UI::new(), &ctx).unwrap();

        assert!(root.join(".dotf/configs").is_dir());

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "missing .dotf/configs/* in .gitignore"
        );
        assert!(
            gitignore.contains("!.dotf/configs/*.tmpl"),
            "missing !.dotf/configs/*.tmpl in .gitignore"
        );
        assert!(
            gitignore.contains(".dotf/.secrets.toml"),
            "missing .dotf/.secrets.toml in .gitignore"
        );
    }

    #[test]
    fn run_local_gitignore_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let ctx = DotfContext::local(root.clone());

        run_local(&UI::new(), &ctx).unwrap();
        run_local(&UI::new(), &ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        let count = gitignore
            .lines()
            .filter(|l| l.trim() == ".dotf/configs/*")
            .count();
        assert_eq!(
            count, 1,
            "gitignore entry duplicated: found {count} occurrences"
        );
    }

    #[test]
    fn run_local_preserves_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        std::fs::write(root.join(".gitignore"), "node_modules/\n*.log\n").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&UI::new(), &ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.contains("node_modules/"),
            "existing .gitignore content was lost"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }

    #[test]
    fn run_local_gitignore_no_trailing_newline() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        std::fs::write(root.join(".gitignore"), "existing").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&UI::new(), &ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.starts_with("existing\n"),
            "missing newline separator: {gitignore:?}"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }

    #[test]
    fn run_local_gitignore_empty_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        std::fs::write(root.join(".gitignore"), "").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&UI::new(), &ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            !gitignore.starts_with('\n'),
            "empty file should not get leading blank line: {gitignore:?}"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }

    #[test]
    fn run_local_creates_dotf_and_gitignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let ctx = DotfContext::local(root.clone());

        run_local(&ctx).unwrap();

        // .dotf/configs/ must exist
        assert!(root.join(".dotf/configs").is_dir());

        // .gitignore must contain the expected entries
        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "missing .dotf/configs/* in .gitignore"
        );
        assert!(
            gitignore.contains("!.dotf/configs/*.tmpl"),
            "missing !.dotf/configs/*.tmpl in .gitignore"
        );
        assert!(
            gitignore.contains(".dotf/.secrets.toml"),
            "missing .dotf/.secrets.toml in .gitignore"
        );
    }

    #[test]
    fn run_local_gitignore_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let ctx = DotfContext::local(root.clone());

        // Run twice
        run_local(&ctx).unwrap();
        run_local(&ctx).unwrap();

        // .gitignore entries must not be duplicated
        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        let count = gitignore
            .lines()
            .filter(|l| l.trim() == ".dotf/configs/*")
            .count();
        assert_eq!(
            count, 1,
            "gitignore entry duplicated: found {count} occurrences"
        );
    }

    #[test]
    fn run_local_preserves_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        // Write a pre-existing .gitignore
        std::fs::write(root.join(".gitignore"), "node_modules/\n*.log\n").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.contains("node_modules/"),
            "existing .gitignore content was lost"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }

    #[test]
    fn run_local_gitignore_no_trailing_newline() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        // Existing .gitignore without trailing newline
        std::fs::write(root.join(".gitignore"), "existing").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        // Must have a newline between existing content and new entries
        assert!(
            gitignore.starts_with("existing\n"),
            "missing newline separator: {gitignore:?}"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }

    #[test]
    fn run_local_gitignore_empty_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        // Empty .gitignore — should not add a leading blank line
        std::fs::write(root.join(".gitignore"), "").unwrap();

        let ctx = DotfContext::local(root.clone());
        run_local(&ctx).unwrap();

        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            !gitignore.starts_with('\n'),
            "empty file should not get leading blank line: {gitignore:?}"
        );
        assert!(
            gitignore.contains(".dotf/configs/*"),
            "dotf entries not added"
        );
    }
}
