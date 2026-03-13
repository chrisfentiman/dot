use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{Input, theme::ColorfulTheme};
use std::fs;
use which::which;

use crate::dotfiles;
use crate::runner::Runner;

pub fn run(runner: &dyn Runner) -> Result<()> {
    println!("{}", "┌─────────────────────────────────────┐".cyan());
    println!("{}", "│        dotf — dotfiles manager       │".cyan());
    println!("{}", "│    secret injection via pass/op/bw   │".cyan());
    println!("{}", "└─────────────────────────────────────┘".cyan());
    println!();

    setup_dotfiles_dir(runner)?;
    hint_secret_backends()?;
    #[cfg(target_os = "macos")]
    run_brewfile(runner)?;
    #[cfg(unix)]
    install_completions(runner)?;

    let synced = dotfiles::render_and_symlink_all()?;

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

fn setup_dotfiles_dir(runner: &dyn Runner) -> Result<()> {
    let dotfiles = dotfiles::dotfiles_dir()?;

    if dotfiles.exists() {
        println!("{} {} already exists", "✓".green(), dotfiles.display());
        return Ok(());
    }

    println!("{}", "~/dotfiles not found.".yellow());

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

fn hint_secret_backends() -> Result<()> {
    let secrets = dotfiles::read_secrets().unwrap_or_default();
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
fn run_brewfile(runner: &dyn Runner) -> Result<()> {
    let brewfile = dotfiles::dotfiles_dir()?.join("Brewfile");
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
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let completions_dir = home.join(".zfunc");
    fs::create_dir_all(&completions_dir).context("Failed to create ~/.zfunc")?;

    let completion_file = completions_dir.join("_dotf");
    let result = runner.run("dotf", &["completions", "zsh"], None);

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
