use anyhow::{Context, Result};
use colored::Colorize;

use crate::dotfiles::{DotfContext, DotfMode};
use crate::runner::Runner;

pub fn run(runner: &dyn Runner, ctx: &DotfContext) -> Result<()> {
    // Local mode: skip all git operations, just render+symlink
    if matches!(&ctx.mode, DotfMode::Local(_)) {
        println!("Re-rendering templates and updating symlinks...");
        let synced = ctx.render_and_symlink_all()?;
        for entry in &synced {
            println!("  {} {}", "✓".green(), entry);
        }
        println!();
        println!(
            "{} Sync complete — {} config(s) up to date",
            "✓".green().bold(),
            synced.len()
        );
        return Ok(());
    }

    let dotfiles_dir = ctx.dotfiles_dir()?;

    println!("Pulling latest changes...");
    let pull = runner.run("git", &["pull", "--rebase"], Some(&dotfiles_dir))?;

    if !pull.stdout.trim().is_empty() {
        println!("{}", pull.stdout.trim());
    }
    if !pull.stderr.trim().is_empty() {
        eprintln!("{}", pull.stderr.trim());
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
            println!("    cd {} && git rebase --continue", dotfiles_dir.display());
            println!("    dotf sync");
            anyhow::bail!("git pull failed due to merge conflicts — resolve manually");
        }

        anyhow::bail!("git pull failed: {}", pull.stderr.trim());
    }

    println!("{} git pull done", "✓".green());

    println!("Re-rendering templates and updating symlinks...");
    let synced = ctx.render_and_symlink_all()?;
    for entry in &synced {
        println!("  {} {}", "✓".green(), entry);
    }

    let now = chrono::Local::now().format("%Y-%m-%d").to_string();
    let commit_msg = format!("chore: sync {now}");

    // Stage all modified tracked files first (covers edits to existing templates,
    // .symlinks.toml, .secrets.toml, etc.).
    let update = runner
        .run("git", &["add", "--update"], Some(&dotfiles_dir))
        .context("Failed to run git add --update")?;
    if !update.success() {
        anyhow::bail!("git add --update failed");
    }

    // Explicitly stage paths that --update skips (new/untracked files).
    // Git interprets pathspecs itself (no shell glob expansion needed).
    let add_new = runner
        .run(
            "git",
            &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
            Some(&dotfiles_dir),
        )
        .context("Failed to run git add for new files")?;
    if !add_new.success() {
        anyhow::bail!("git add failed");
    }

    let commit = runner
        .run("git", &["commit", "-m", &commit_msg], Some(&dotfiles_dir))
        .context("Failed to run git commit")?;

    if !commit.success() {
        let nothing = commit.stdout.contains("nothing to commit")
            || commit.stderr.contains("nothing to commit");
        if nothing {
            println!("{} Nothing new to commit", "·".dimmed());
        } else {
            anyhow::bail!(
                "git commit failed: {}",
                commit.stderr.trim().lines().next().unwrap_or("unknown error")
            );
        }
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
    use tempfile::TempDir;

    fn ctx() -> DotfContext {
        DotfContext::global()
    }

    /// Set up a minimal temp HOME with dotfiles dir so `render_and_symlink_all()`
    /// succeeds (returns empty vec).
    struct SyncEnv {
        // Drop order: _home_guard restores HOME, _tmp deletes dir, _lock releases mutex.
        _home_guard: crate::EnvGuard,
        _tmp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    fn sync_env() -> SyncEnv {
        let lock = crate::env_lock();
        let tmp = TempDir::new().unwrap();
        let dotfiles = tmp.path().join("dotfiles");
        std::fs::create_dir_all(dotfiles.join("configs")).unwrap();
        std::fs::write(dotfiles.join(".symlinks.toml"), "[symlinks]\n").unwrap();
        std::fs::write(dotfiles.join(".secrets.toml"), "[secrets]\n").unwrap();
        let home_guard = crate::EnvGuard::set("HOME", &tmp.path().to_string_lossy());
        SyncEnv { _home_guard: home_guard, _tmp: tmp, _lock: lock }
    }

    fn today_commit_msg() -> String {
        let now = chrono::Local::now().format("%Y-%m-%d").to_string();
        format!("chore: sync {now}")
    }

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

        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("merge conflicts"));
    }

    #[test]
    fn sync_pull_failure_no_conflicts_bails() {
        let _g = crate::env_lock();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", false)
            .on("git", &["diff", "--name-only", "--diff-filter=U"], "", true);

        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("git pull failed"));
    }

    #[test]
    fn sync_full_success() {
        let _env = sync_env();
        let msg = today_commit_msg();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "Already up to date.\n", true)
            .on("git", &["add", "--update"], "", true)
            .on(
                "git",
                &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
                "",
                true,
            )
            .on("git", &["commit", "-m", &msg], "", true)
            .on("git", &["push"], "", true);
        run(&runner, &ctx()).unwrap();
    }

    #[test]
    fn sync_nothing_to_commit() {
        let _env = sync_env();
        let msg = today_commit_msg();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", true)
            .on("git", &["add", "--update"], "", true)
            .on(
                "git",
                &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
                "",
                true,
            )
            .on_err("git", &["commit", "-m", &msg], "nothing to commit, working tree clean", false);
        run(&runner, &ctx()).unwrap();
    }

    #[test]
    fn sync_git_add_update_failure_bails() {
        let _env = sync_env();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", true)
            .on("git", &["add", "--update"], "", false);
        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("git add"));
    }

    #[test]
    fn sync_git_add_new_failure_bails() {
        let _env = sync_env();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", true)
            .on("git", &["add", "--update"], "", true)
            .on(
                "git",
                &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
                "",
                false,
            );
        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("git add failed"));
    }

    #[test]
    fn sync_git_push_failure_bails() {
        let _env = sync_env();
        let msg = today_commit_msg();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", true)
            .on("git", &["add", "--update"], "", true)
            .on(
                "git",
                &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
                "",
                true,
            )
            .on("git", &["commit", "-m", &msg], "", true)
            .on_err("git", &["push"], "remote rejected", false);
        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("git push failed"));
    }

    #[test]
    fn sync_commit_failure_not_nothing_to_commit_bails() {
        let _env = sync_env();
        let msg = today_commit_msg();
        let runner = MockRunner::new()
            .on("git", &["pull", "--rebase"], "", true)
            .on("git", &["add", "--update"], "", true)
            .on(
                "git",
                &["add", "configs/", ".symlinks.toml", ".secrets.toml"],
                "",
                true,
            )
            .on_err("git", &["commit", "-m", &msg], "some other error", false);
        let err = run(&runner, &ctx()).unwrap_err();
        assert!(err.to_string().contains("git commit failed"));
    }

    #[test]
    fn sync_local_mode_skips_git() {
        // Local mode should never call any git commands — MockRunner with no
        // registrations (and no allow_unmatched) will panic if git is invoked.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dotf_dir = root.join(".dotf");
        let configs = dotf_dir.join("configs");
        std::fs::create_dir_all(&configs).unwrap();
        std::fs::write(dotf_dir.join(".symlinks.toml"), "[symlinks]\n").unwrap();
        std::fs::write(dotf_dir.join(".secrets.toml"), "[secrets]\n").unwrap();

        let ctx = DotfContext::local(root.to_path_buf());
        let runner = MockRunner::new(); // panics on any call
        run(&runner, &ctx).unwrap();
    }
}
