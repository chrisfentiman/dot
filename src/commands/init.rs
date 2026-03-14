use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{Input, theme::ColorfulTheme};
use std::fs;
use which::which;

use crate::dotfiles;
use crate::dotfiles::{DotfContext, DotfMode};
use crate::runner::Runner;

pub fn run(runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    if matches!(&ctx.mode, DotfMode::Local(_)) {
        return run_local(ctx);
    }

    println!("{}", "┌─────────────────────────────────────┐".cyan());
    println!("{}", "│        dotf — dotfiles manager       │".cyan());
    println!("{}", "│    secret injection via pass/op/bw   │".cyan());
    println!("{}", "└─────────────────────────────────────┘".cyan());
    println!();

    setup_dotfiles_dir(runner, ctx)?;
    hint_secret_backends(ctx)?;
    #[cfg(target_os = "macos")]
    run_brewfile(runner, ctx)?;
    #[cfg(unix)]
    install_completions(runner)?;

    let synced = ctx.render_and_symlink_all()?;

    println!();
    println!("{}", "Setup complete!".green().bold());
    if synced.is_empty() {
        println!(
            "  No configs symlinked yet. Run {} to add one.",
            "dotf config <path>".cyan()
        );
    } else {
        println!("  Symlinked configs:");
        for entry in &synced {
            println!("    {} {}", "✓".green(), entry);
        }
    }

    Ok(())
}

fn run_local(ctx: &DotfContext) -> Result<()> {
    let dotf_dir = ctx.dotfiles_dir()?;
    let configs_dir = ctx.configs_dir()?;

    // Warn if a parent directory already has .dotf/ — creating a nested one
    // is almost certainly a mistake.
    if let DotfMode::Local(root) = &ctx.mode
        && let Some(parent) = root.parent()
        && let Some(existing) = dotfiles::find_dotf_root(parent)
    {
        println!(
            "{} A .dotf/ directory already exists at {}",
            "!".yellow(),
            existing.display()
        );
        println!(
            "  Creating a nested .dotf/ at {} — is this intentional?",
            root.display()
        );
        println!();
    }

    fs::create_dir_all(&configs_dir)
        .with_context(|| format!("Failed to create {}", configs_dir.display()))?;
    println!("{} Created {}", "✓".green(), dotf_dir.display());

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
        println!(
            "{} Updated .gitignore ({} entries added)",
            "✓".green(),
            to_add.len()
        );
    } else {
        println!("{} .gitignore already configured", "✓".green());
    }

    let synced = ctx.render_and_symlink_all()?;

    println!();
    println!("{}", "Local dotf setup complete!".green().bold());
    if synced.is_empty() {
        println!(
            "  No configs yet. Run {} to add one.",
            "dotf config <path>".cyan()
        );
    } else {
        println!("  Symlinked configs:");
        for entry in &synced {
            println!("    {} {}", "✓".green(), entry);
        }
    }

    Ok(())
}

fn setup_dotfiles_dir(runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    let dotfiles = ctx.dotfiles_dir()?;

    if dotfiles.exists() {
        println!("{} {} already exists", "✓".green(), dotfiles.display());
        return Ok(());
    }

    // Migration hint: if old ~/dotfiles exists, tell the user.
    // We know ~/.dotf doesn't exist here (early return above).
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_dotfiles = home.join("dotfiles");
    if old_dotfiles.exists() {
        println!(
            "{} Found existing ~/dotfiles — dotf now uses ~/.dotf/",
            "!".yellow()
        );
        println!("  Run: mv ~/dotfiles ~/.dotf");
        println!();
    }

    println!("{}", "~/.dotf not found.".yellow());

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

        println!(
            "{} Created fresh dotfiles repo at {}",
            "✓".green(),
            dotfiles.display()
        );

        if which("gh").is_ok() {
            println!("Creating private GitHub repo dotfiles...");
            let source = dotfiles
                .to_str()
                .ok_or_else(|| anyhow!("dotfiles path is not valid UTF-8"))?;
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
                println!("{} GitHub repo created and remote set", "✓".green());
            } else {
                println!(
                    "{} gh repo create failed — you can set up the remote manually",
                    "!".yellow()
                );
            }
        } else {
            println!(
                "{} gh not found — skipping GitHub repo creation",
                "!".yellow()
            );
        }
    } else {
        println!("Cloning {}...", url);
        let dotfiles_str = dotfiles
            .to_str()
            .ok_or_else(|| anyhow!("dotfiles path is not valid UTF-8"))?;
        let clone = runner.run("git", &["clone", &url, dotfiles_str], None)?;
        if !clone.success() {
            anyhow::bail!("git clone failed");
        }
        println!("{} Cloned dotfiles to {}", "✓".green(), dotfiles.display());
    }

    Ok(())
}

fn hint_secret_backends(ctx: &DotfContext) -> Result<()> {
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
        println!(
            "{} No secret backends configured yet — add secrets with {}",
            "·".dimmed(),
            "dotf secrets add".cyan()
        );
        return Ok(());
    }

    if needs_pass {
        #[cfg(target_os = "macos")]
        check_cli("pass", "Proton Pass", "brew install protonpass/pass/pass")?;
        #[cfg(not(target_os = "macos"))]
        check_cli("pass", "Proton Pass", "https://proton.me/pass/download")?;
    }
    if needs_op {
        #[cfg(target_os = "macos")]
        check_cli("op", "1Password", "brew install 1password-cli")?;
        #[cfg(not(target_os = "macos"))]
        check_cli(
            "op",
            "1Password",
            "https://developer.1password.com/docs/cli/get-started/",
        )?;
    }
    if needs_bw {
        #[cfg(target_os = "macos")]
        check_cli("bw", "Bitwarden", "brew install bitwarden-cli")?;
        #[cfg(not(target_os = "macos"))]
        check_cli("bw", "Bitwarden", "npm install -g @bitwarden/cli")?;
    }

    Ok(())
}

fn check_cli(bin: &str, name: &str, install_hint: &str) -> Result<()> {
    if which(bin).is_ok() {
        println!("{} {} CLI ({}) available", "✓".green(), name, bin);
    } else {
        println!(
            "{} {} CLI (`{}`) not found — install with: {}",
            "!".yellow(),
            name,
            bin,
            install_hint.cyan()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_brewfile(runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    let brewfile = ctx.dotfiles_dir()?.join("Brewfile");
    if !brewfile.exists() {
        println!("{} No Brewfile found, skipping brew bundle", "·".dimmed());
        return Ok(());
    }

    println!("Running brew bundle...");
    let brewfile_str = brewfile
        .to_str()
        .ok_or_else(|| anyhow!("Brewfile path is not valid UTF-8"))?;
    let result = runner.run(
        "brew",
        &["bundle", "install", &format!("--file={brewfile_str}")],
        None,
    )?;
    if !result.success() {
        anyhow::bail!("brew bundle install failed");
    }
    println!("{} brew bundle complete", "✓".green());
    Ok(())
}

#[cfg(unix)]
fn install_completions(runner: &dyn Runner) -> Result<()> {
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
            println!(
                "{} Zsh completions installed to {}",
                "✓".green(),
                completion_file.display()
            );
            println!(
                "  Add to ~/.zshrc if not already present: fpath=(~/.zfunc $fpath) && autoload -U compinit && compinit"
            );
        }
        _ => {
            println!(
                "{} Could not install completions (dotf not in PATH yet — run after brew install)",
                "·".dimmed()
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
        run(&runner, &ctx).unwrap();
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
