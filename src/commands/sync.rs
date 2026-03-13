use anyhow::{Context, Result};
use colored::Colorize;

use crate::dotfiles;
use crate::runner::Runner;

pub fn run(runner: &dyn Runner) -> Result<()> {
    let dotfiles_dir = dotfiles::dotfiles_dir()?;

    println!("Pulling latest changes...");
    let pull = runner.run("git", &["pull", "--rebase"], Some(&dotfiles_dir))?;

    if !pull.stdout.trim().is_empty() {
        println!("{}", pull.stdout.trim());
    }
    if !pull.stderr.trim().is_empty() {
        println!("{}", pull.stderr.trim().dimmed());
    }

    if !pull.success() {
        let conflicts =
            runner.run("git", &["diff", "--name-only", "--diff-filter=U"], Some(&dotfiles_dir))?;

        if !conflicts.stdout.trim().is_empty() {
            println!();
            println!("{} Merge conflicts detected in:", "✗".red().bold());
            for file in conflicts.stdout.trim().lines() {
                println!("    {}", file.yellow());
            }
            println!();
            println!("Resolve conflicts, then run:");
            println!("    cd ~/dotfiles && git rebase --continue");
            println!("    dotf sync");
            anyhow::bail!("git pull failed due to merge conflicts — resolve manually");
        }

        anyhow::bail!("git pull failed: {}", pull.stderr.trim());
    }

    println!("{} git pull done", "✓".green());

    println!("Re-rendering templates and updating symlinks...");
    let synced = dotfiles::render_and_symlink_all()?;
    for entry in &synced {
        println!("  {} {}", "✓".green(), entry);
    }

    let now = chrono::Local::now().format("%Y-%m-%d").to_string();
    let commit_msg = format!("sync: {now}");

    let add = runner.run("git", &["add", "--update"], Some(&dotfiles_dir))
        .context("Failed to run git add")?;
    if !add.success() {
        anyhow::bail!("git add failed");
    }

    let commit = runner
        .run("git", &["commit", "-m", &commit_msg], Some(&dotfiles_dir))
        .context("Failed to run git commit")?;

    if commit.stdout.contains("nothing to commit") || !commit.success() {
        println!("{} Nothing new to commit", "·".dimmed());
    } else {
        println!("{} Committed: {}", "✓".green(), commit_msg);

        let push = runner
            .run("git", &["push"], Some(&dotfiles_dir))
            .context("Failed to run git push")?;
        if !push.success() {
            anyhow::bail!("git push failed: {}", push.stderr.trim());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MockRunner;

    #[test]
    fn sync_pull_conflict_shows_instructions() {
        let _g = crate::env_lock();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", false)
            .on(
                "git",
                &["diff", "--name-only", "--diff-filter=U"],
                ".secrets.toml\n",
                true,
            );

        let err = run(&runner).unwrap_err();
        assert!(err.to_string().contains("merge conflicts"));
    }

    #[test]
    fn sync_pull_failure_no_conflicts_bails() {
        let _g = crate::env_lock();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", false)
            .on("git", &["diff", "--name-only", "--diff-filter=U"], "", true);

        let err = run(&runner).unwrap_err();
        assert!(err.to_string().contains("git pull failed"));
    }
}
