use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Input, theme::ColorfulTheme};
use std::fs;
use std::process::Command;
use which::which;

use crate::dotfiles;

pub fn run() -> Result<()> {
    println!("{}", "┌─────────────────────────────────────┐".cyan());
    println!("{}", "│        dotf — dotfiles manager       │".cyan());
    println!("{}", "│    secret injection via pass/op/bw   │".cyan());
    println!("{}", "└─────────────────────────────────────┘".cyan());
    println!();

    setup_dotfiles_dir()?;
    hint_secret_backends()?;
    #[cfg(target_os = "macos")]
    run_brewfile()?;
    #[cfg(unix)]
    install_completions()?;

    let synced = dotfiles::render_and_symlink_all()?;

    println!();
    println!("{}", "Setup complete!".green().bold());
    if synced.is_empty() {
        println!(
            "  No configs symlinked yet. Run {} to add one.",
            "dot config <path>".cyan()
        );
    } else {
        println!("  Symlinked configs:");
        for entry in &synced {
            println!("    {} {}", "✓".green(), entry);
        }
    }

    Ok(())
}

fn setup_dotfiles_dir() -> Result<()> {
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

        let init_status = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&dotfiles)
            .status()
            .context("Failed to run git init")?;
        if !init_status.success() {
            Command::new("git")
                .args(["init"])
                .current_dir(&dotfiles)
                .status()
                .context("Failed to run git init")?;
        }

        fs::create_dir_all(dotfiles.join("configs")).context("Failed to create configs dir")?;

        println!(
            "{} Created fresh dotfiles repo at {}",
            "✓".green(),
            dotfiles.display()
        );

        if which("gh").is_ok() {
            println!("Creating private GitHub repo dotfiles...");
            let source = dotfiles.to_string_lossy();
            let gh_status = Command::new("gh")
                .args([
                    "repo",
                    "create",
                    "dotfiles",
                    "--private",
                    &format!("--source={}", source),
                    "--remote=origin",
                ])
                .status()
                .context("Failed to run gh repo create")?;
            if gh_status.success() {
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
        let clone_status = Command::new("git")
            .args(["clone", &url, dotfiles.to_str().unwrap()])
            .status()
            .context("Failed to run git clone")?;
        if !clone_status.success() {
            anyhow::bail!("git clone failed");
        }
        println!("{} Cloned dotfiles to {}", "✓".green(), dotfiles.display());
    }

    Ok(())
}

fn hint_secret_backends() -> Result<()> {
    // Read which backends are actually used in the secrets file and check their CLIs
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

    // If no secrets configured yet, nothing to check
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
        check_cli("op", "1Password", "https://developer.1password.com/docs/cli/get-started/")?;
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
fn run_brewfile() -> Result<()> {
    let brewfile = dotfiles::dotfiles_dir()?.join("Brewfile");
    if !brewfile.exists() {
        println!("{} No Brewfile found, skipping brew bundle", "·".dimmed());
        return Ok(());
    }

    println!("Running brew bundle...");
    let status = Command::new("brew")
        .args([
            "bundle",
            "install",
            &format!("--file={}", brewfile.display()),
        ])
        .status()
        .context("Failed to run brew bundle")?;
    if !status.success() {
        anyhow::bail!("brew bundle install failed");
    }
    println!("{} brew bundle complete", "✓".green());
    Ok(())
}

#[cfg(unix)]
fn install_completions() -> Result<()> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let completions_dir = home.join(".zfunc");
    fs::create_dir_all(&completions_dir).context("Failed to create ~/.zfunc")?;

    let completion_file = completions_dir.join("_dotf");
    let output = Command::new("dotf").args(["completions", "zsh"]).output();

    match output {
        Ok(out) if out.status.success() => {
            fs::write(&completion_file, out.stdout)
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
