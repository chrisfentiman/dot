use anyhow::{Context, Result};
use dialoguer::{Select, theme::ColorfulTheme};
use similar::{ChangeTag, TextDiff};
use std::fs;

use crate::dotfiles;
use crate::dotfiles::DotfContext;
use crate::ui::UI;

pub fn run(ui: &UI, ctx: &DotfContext, name: Option<String>) -> Result<()> {
    let symlinks = ctx.read_symlinks()?;

    if symlinks.symlinks.is_empty() {
        anyhow::bail!("No managed configs found.");
    }

    let names_to_diff: Vec<String> = match name {
        Some(n) => {
            if !symlinks.symlinks.contains_key(&n) {
                anyhow::bail!("Config '{}' is not managed by dotf", n);
            }
            vec![n]
        }
        None => {
            let mut names: Vec<String> = symlinks.symlinks.keys().cloned().collect();
            names.sort();
            names.insert(0, "(all)".to_string());
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select config to diff (or all)")
                .items(&names)
                .default(0)
                .interact()
                .context("Failed to read selection")?;
            if selection == 0 {
                // all — remove the "(all)" sentinel
                names.into_iter().skip(1).collect()
            } else {
                vec![names[selection].clone()]
            }
        }
    };

    let secrets = ctx.read_secrets()?;
    let configs_dir = ctx.configs_dir()?;
    let mut any_diff = false;

    for config_name in &names_to_diff {
        let template_path = configs_dir.join(format!("{config_name}.tmpl"));
        let rendered_path = configs_dir.join(config_name);

        if !template_path.exists() {
            ui.warn(
                "Skipped",
                format!("{}: template not found", ui.highlight(config_name)),
            );
            continue;
        }

        // Re-render fresh from template+secrets (in memory, don't write)
        let fresh = match dotfiles::render_template(&template_path, &secrets) {
            Ok(s) => s,
            Err(e) => {
                ui.error(
                    "Error",
                    format!("{}: failed to render — {}", ui.highlight(config_name), e),
                );
                continue;
            }
        };

        let current = if rendered_path.exists() {
            fs::read_to_string(&rendered_path)
                .with_context(|| format!("Failed to read {}", rendered_path.display()))?
        } else {
            String::new()
        };

        if fresh == current {
            ui.skip(
                "Checking",
                format!("{} — no changes", ui.highlight(config_name)),
            );
        } else {
            any_diff = true;
            ui.blank();
            ui.action("Diffing", ui.highlight(config_name));

            for line in compute_diff(&current, &fresh) {
                ui.raw(&line);
            }
            ui.blank();
        }
    }

    if !any_diff && names_to_diff.len() > 1 {
        ui.blank();
        ui.finished("all configs are up to date");
    }

    Ok(())
}

/// Returns a human-readable unified-style diff of two text strings.
/// Uses `TextDiff::from_lines` so newline termination is handled correctly.
/// Exposed `pub` so fuzz targets and tests can call it without going through I/O.
pub fn compute_diff(old: &str, new: &str) -> Vec<String> {
    let diff = TextDiff::from_lines(old, new);
    diff.iter_all_changes()
        .map(|change| {
            let line = change.value().trim_end_matches(['\n', '\r']);
            match change.tag() {
                ChangeTag::Equal => format!("  {line}"),
                ChangeTag::Insert => format!("+ {line}"),
                ChangeTag::Delete => format!("- {line}"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_diff_equal_inputs() {
        let result = compute_diff("a\nb\n", "a\nb\n");
        assert!(
            !result.is_empty(),
            "equal inputs should still produce context lines"
        );
        for line in &result {
            assert!(
                line.starts_with("  "),
                "equal input lines should be context: {line:?}"
            );
        }
    }

    #[test]
    fn compute_diff_insertion() {
        let result = compute_diff("a\n", "a\nb\n");
        let inserts: Vec<_> = result.iter().filter(|l| l.starts_with("+ ")).collect();
        assert_eq!(inserts.len(), 1);
        assert_eq!(inserts[0], "+ b");
    }

    #[test]
    fn compute_diff_deletion() {
        let result = compute_diff("a\nb\n", "a\n");
        let deletes: Vec<_> = result.iter().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(deletes.len(), 1);
        assert_eq!(deletes[0], "- b");
    }

    #[test]
    fn compute_diff_replacement() {
        let result = compute_diff("old\n", "new\n");
        let deletes: Vec<_> = result.iter().filter(|l| l.starts_with("- ")).collect();
        let inserts: Vec<_> = result.iter().filter(|l| l.starts_with("+ ")).collect();
        assert_eq!(deletes.len(), 1);
        assert_eq!(inserts.len(), 1);
        assert_eq!(deletes[0], "- old");
        assert_eq!(inserts[0], "+ new");
    }

    #[test]
    fn compute_diff_empty_inputs() {
        let result = compute_diff("", "");
        assert!(result.is_empty());
    }

    #[test]
    fn compute_diff_all_lines_have_valid_prefix() {
        let result = compute_diff("a\nb\nc\n", "a\nX\nc\n");
        for line in &result {
            assert!(
                line.starts_with("  ") || line.starts_with("+ ") || line.starts_with("- "),
                "invalid prefix: {line:?}"
            );
        }
    }

    #[test]
    fn compute_diff_handles_crlf() {
        let result = compute_diff("a\r\n", "a\r\n");
        for line in &result {
            assert!(!line.contains('\r'), "\\r should be stripped: {line:?}");
        }
    }
}
