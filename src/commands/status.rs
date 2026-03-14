use anyhow::Result;
use std::fs;

use crate::dotfiles::DotfContext;
use crate::ui::UI;

#[derive(Debug, PartialEq)]
pub(crate) enum ConfigStatus {
    Ok,
    MissingSymlink,
    BrokenSymlink,
    MissingTemplate,
    WrongTarget(String),
}

pub(crate) fn check_config_status(
    ctx: &DotfContext,
    template_path: &std::path::Path,
    output_path: &std::path::Path,
    target_str: &str,
) -> ConfigStatus {
    if !template_path.exists() {
        return ConfigStatus::MissingTemplate;
    }
    match ctx.resolve_symlink_target(target_str) {
        Err(_) => ConfigStatus::MissingSymlink,
        Ok(link_path) => {
            if link_path.symlink_metadata().is_err() {
                return ConfigStatus::MissingSymlink;
            }
            match fs::read_link(&link_path) {
                Err(_) => {
                    // Path exists but isn't a symlink (regular file, directory, etc.)
                    ConfigStatus::BrokenSymlink
                }
                Ok(dest) => {
                    // Symlink exists — check if its target is reachable.
                    // A dangling symlink (target deleted) has symlink_metadata() Ok
                    // but the target path won't exist or canonicalize.
                    if !dest.exists() && link_path.canonicalize().is_err() {
                        return ConfigStatus::BrokenSymlink;
                    }
                    let dest_c = dest.canonicalize().unwrap_or(dest.clone());
                    let expected_c = output_path
                        .canonicalize()
                        .unwrap_or(output_path.to_path_buf());
                    if dest_c == expected_c {
                        ConfigStatus::Ok
                    } else {
                        ConfigStatus::WrongTarget(dest.display().to_string())
                    }
                }
            }
        }
    }
}

/// Collect the status of each managed config. Returns a sorted vec of
/// (config_name, target_path, status). Separated from printing so tests
/// can assert on the data without capturing stdout.
pub(crate) fn collect_statuses(ctx: &DotfContext) -> Result<Vec<(String, String, ConfigStatus)>> {
    let symlinks = ctx.read_symlinks()?;
    let configs_dir = ctx.configs_dir()?;

    let mut entries: Vec<_> = symlinks.symlinks.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut results = Vec::new();
    for (name, target_str) in &entries {
        let template_path = configs_dir.join(format!("{name}.tmpl"));
        let output_path = configs_dir.join(name);
        let status = check_config_status(ctx, &template_path, &output_path, target_str);
        results.push((name.clone(), target_str.clone(), status));
    }
    Ok(results)
}

pub fn run(ui: &UI, ctx: &DotfContext) -> Result<()> {
    let statuses = collect_statuses(ctx)?;

    if statuses.is_empty() {
        ui.skip(
            "Info",
            format!(
                "No managed configs. Run {} to add one.",
                ui.highlight("dotf config <path>")
            ),
        );
        return Ok(());
    }

    let name_width = statuses
        .iter()
        .map(|(k, _, _)| k.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let target_width = statuses
        .iter()
        .map(|(_, v, _)| v.len())
        .max()
        .unwrap_or(6)
        .max(6);

    ui.table_header(&[
        ("CONFIG", name_width),
        ("TARGET", target_width),
        ("STATUS", 6),
    ]);
    ui.table_separator(name_width + target_width + 10);

    for (name, target_str, status) in &statuses {
        let (symbol, label) = match status {
            ConfigStatus::Ok => (ui.sym_ok(), "ok".to_string()),
            ConfigStatus::MissingSymlink => (ui.sym_warn(), "missing symlink".to_string()),
            ConfigStatus::BrokenSymlink => (ui.sym_err(), "broken symlink".to_string()),
            ConfigStatus::MissingTemplate => (ui.sym_err(), "missing template".to_string()),
            ConfigStatus::WrongTarget(t) => (ui.sym_err(), format!("wrong target: {t}")),
        };

        ui.table_row(format!(
            "{:<name_width$}  {:<target_width$}  {symbol} {label}",
            ui.highlight(name),
            ui.dim(target_str),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotfiles::SymlinksFile;
    use std::collections::HashMap;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    struct Env {
        // Drop order: _home_guard restores HOME, _tmp deletes dir, _lock releases mutex.
        _home_guard: crate::EnvGuard,
        _tmp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new() -> Self {
            let _lock = crate::env_lock();
            let tmp = TempDir::new().unwrap();
            let dotfiles = tmp.path().join(".dotf");
            std::fs::create_dir_all(dotfiles.join("configs")).unwrap();
            let _home_guard = crate::EnvGuard::set("HOME", &tmp.path().to_string_lossy());
            Env {
                _tmp: tmp,
                _home_guard,
                _lock,
            }
        }

        fn dotfiles(&self) -> std::path::PathBuf {
            self._tmp.path().join(".dotf")
        }
    }

    fn write_symlinks_map(env: &Env, entries: &[(&str, &str)]) {
        let mut map = HashMap::new();
        for (k, v) in entries {
            map.insert(k.to_string(), v.to_string());
        }
        let sf = SymlinksFile { symlinks: map };
        let path = env.dotfiles().join(".symlinks.toml");
        std::fs::write(&path, toml::to_string_pretty(&sf).unwrap()).unwrap();
    }

    fn ctx() -> DotfContext {
        DotfContext::global()
    }

    // ── check_config_status direct tests ───────────────────────
    #[test]
    fn check_status_ok_correct_symlink() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();
        std::fs::write(configs.join("cfg"), "x").unwrap();

        let link = env._tmp.path().join("cfg");
        symlink(configs.join("cfg"), &link).unwrap();

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert_eq!(status, ConfigStatus::Ok);
    }

    #[test]
    fn check_status_wrong_target() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();
        std::fs::write(configs.join("cfg"), "x").unwrap();

        let elsewhere = env._tmp.path().join("other");
        std::fs::write(&elsewhere, "y").unwrap();
        let link = env._tmp.path().join("cfg");
        symlink(&elsewhere, &link).unwrap();

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert!(matches!(status, ConfigStatus::WrongTarget(_)));
    }

    #[test]
    fn check_status_missing_template() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        let link = env._tmp.path().join("cfg");

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert_eq!(status, ConfigStatus::MissingTemplate);
    }

    #[test]
    fn check_status_missing_symlink() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();
        // link doesn't exist
        let link = env._tmp.path().join("cfg");

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert_eq!(status, ConfigStatus::MissingSymlink);
    }

    #[test]
    fn check_status_broken_symlink_regular_file() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();

        let link = env._tmp.path().join("cfg");
        std::fs::write(&link, "regular").unwrap();

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert_eq!(status, ConfigStatus::BrokenSymlink);
    }

    #[test]
    fn check_status_broken_symlink_dangling() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();

        let link = env._tmp.path().join("cfg");
        symlink("/nonexistent/path/that/does/not/exist", &link).unwrap();

        let status = check_config_status(
            &ctx(),
            &configs.join("cfg.tmpl"),
            &configs.join("cfg"),
            &link.to_string_lossy(),
        );
        assert_eq!(status, ConfigStatus::BrokenSymlink);
    }

    // ── run() / collect_statuses() integration tests ───────────

    #[test]
    fn status_empty_symlinks_returns_empty() {
        let _env = Env::new();
        let statuses = collect_statuses(&ctx()).unwrap();
        assert!(statuses.is_empty());
        run(&UI::new(), &ctx()).unwrap();
    }

    #[test]
    fn status_missing_template_detected() {
        let env = Env::new();
        let link = env._tmp.path().join("link.conf");
        write_symlinks_map(&env, &[("cfg", &link.to_string_lossy())]);

        let statuses = collect_statuses(&ctx()).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "cfg");
        assert_eq!(statuses[0].2, ConfigStatus::MissingTemplate);
    }

    #[test]
    fn status_ok_with_correct_symlink() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");

        std::fs::write(configs.join("cfg.tmpl"), "x = 1").unwrap();
        std::fs::write(configs.join("cfg"), "x = 1").unwrap();

        let link = env._tmp.path().join("cfg");
        symlink(configs.join("cfg"), &link).unwrap();

        write_symlinks_map(&env, &[("cfg", &link.to_string_lossy())]);

        let statuses = collect_statuses(&ctx()).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].2, ConfigStatus::Ok);
    }

    #[test]
    fn status_wrong_target_detected() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");

        std::fs::write(configs.join("cfg.tmpl"), "x = 1").unwrap();
        std::fs::write(configs.join("cfg"), "x = 1").unwrap();

        let elsewhere = env._tmp.path().join("other");
        std::fs::write(&elsewhere, "other").unwrap();
        let link = env._tmp.path().join("cfg");
        symlink(&elsewhere, &link).unwrap();

        write_symlinks_map(&env, &[("cfg", &link.to_string_lossy())]);

        let statuses = collect_statuses(&ctx()).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(matches!(statuses[0].2, ConfigStatus::WrongTarget(_)));
    }

    #[test]
    fn run_with_configs_succeeds() {
        let env = Env::new();
        let configs = env.dotfiles().join("configs");
        std::fs::write(configs.join("cfg.tmpl"), "x").unwrap();
        std::fs::write(configs.join("cfg"), "x").unwrap();

        let link = env._tmp.path().join("cfg");
        symlink(configs.join("cfg"), &link).unwrap();

        write_symlinks_map(&env, &[("cfg", &link.to_string_lossy())]);
        run(&UI::new(), &ctx()).unwrap();
    }

    #[test]
    fn run_propagates_corrupted_symlinks_error() {
        let env = Env::new();
        let path = env.dotfiles().join(".symlinks.toml");
        std::fs::write(&path, "not valid toml {{{{").unwrap();
        let err = run(&UI::new(), &ctx()).unwrap_err();
        assert!(
            err.to_string().contains("parse") || err.to_string().contains("TOML"),
            "run() should propagate parse error: {err}"
        );
    }
}
