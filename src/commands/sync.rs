use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::dotfiles;

pub fn run() -> Result<()> {
    let dotfiles_dir = dotfiles::dotfiles_dir()?;

    println!("Pulling latest changes...");
    let pull_output = Command::new("git")
        .args(["pull"])
        .current_dir(&dotfiles_dir)
        .output()
        .context("Failed to run git pull")?;

    let pull_stdout = String::from_utf8_lossy(&pull_output.stdout);
    let pull_stderr = String::from_utf8_lossy(&pull_output.stderr);
    if !pull_stdout.trim().is_empty() {
        println!("{}", pull_stdout.trim());
    }
    if !pull_stderr.trim().is_empty() {
        println!("{}", pull_stderr.trim().dimmed());
    }
    if !pull_output.status.success() {
        anyhow::bail!("git pull failed");
    }
    println!("{} git pull done", "✓".green());

    println!("Re-rendering templates and updating symlinks...");
    let synced = dotfiles::render_and_symlink_all()?;
    for entry in &synced {
        println!("  {} {}", "✓".green(), entry);
    }

    let now = chrono::Local::now().format("%Y-%m-%d").to_string();
    let commit_msg = format!("sync: {now}");

    let add_output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&dotfiles_dir)
        .output()
        .context("Failed to run git add")?;
    if !add_output.status.success() {
        anyhow::bail!("git add failed");
    }

    let commit_output = Command::new("git")
        .args(["commit", "-m", &commit_msg])
        .current_dir(&dotfiles_dir)
        .output()
        .context("Failed to run git commit")?;

    let commit_stdout = String::from_utf8_lossy(&commit_output.stdout);
    if commit_stdout.contains("nothing to commit") || !commit_output.status.success() {
        println!("{} Nothing new to commit", "·".dimmed());
    } else {
        println!("{} Committed: {}", "✓".green(), commit_msg);

        let push_output = Command::new("git")
            .args(["push"])
            .current_dir(&dotfiles_dir)
            .output()
            .context("Failed to run git push")?;
        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            anyhow::bail!("git push failed: {stderr}");
        }
        println!("{} Pushed to remote", "✓".green());
    }

    println!();
    println!(
        "{} Sync complete — {} config(s) up to date",
        "✓".green().bold(),
        synced.len()
    );
    Ok(())
}
