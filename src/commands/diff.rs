use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Select, theme::ColorfulTheme};
use std::fs;

use crate::dotfiles;

pub fn run(name: Option<String>) -> Result<()> {
    let symlinks = dotfiles::read_symlinks()?;

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

    let secrets = dotfiles::read_secrets()?;
    let configs_dir = dotfiles::configs_dir()?;
    let mut any_diff = false;

    for config_name in &names_to_diff {
        let template_path = configs_dir.join(format!("{config_name}.tmpl"));
        let rendered_path = configs_dir.join(config_name);

        if !template_path.exists() {
            println!(
                "{} {}: template not found, skipping",
                "!".yellow(),
                config_name.cyan()
            );
            continue;
        }

        // Re-render fresh from template+secrets (in memory, don't write)
        let fresh = match dotfiles::render_template(&template_path, &secrets) {
            Ok(s) => s,
            Err(e) => {
                println!(
                    "{} {}: failed to render — {}",
                    "✗".red(),
                    config_name.cyan(),
                    e
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
            println!("{} {} — no changes", "✓".green(), config_name.cyan());
        } else {
            any_diff = true;
            println!();
            println!(
                "{} {}",
                "━━".cyan(),
                format!(" diff: {} ", config_name).cyan().bold()
            );

            // Simple line-by-line diff output
            let old_lines: Vec<&str> = current.lines().collect();
            let new_lines: Vec<&str> = fresh.lines().collect();

            print_diff(&old_lines, &new_lines);
            println!();
        }
    }

    if !any_diff && names_to_diff.len() > 1 {
        println!();
        println!("{} All configs are up to date", "✓".green().bold());
    }

    Ok(())
}

fn print_diff(old: &[&str], new: &[&str]) {
    for line in compute_diff(old, new) {
        println!("{line}");
    }
}

/// Returns a human-readable diff of two line slices.
/// Exposed `pub` so fuzz targets and tests can call it without going through I/O.
pub fn compute_diff(old: &[&str], new: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut oi = 0usize;
    let mut ni = 0usize;

    while oi < old.len() || ni < new.len() {
        if oi < old.len() && ni < new.len() && old[oi] == new[ni] {
            out.push(format!("  {}", old[oi]));
            oi += 1;
            ni += 1;
        } else {
            let lookahead = 4;
            let mut matched = false;
            for delta in 1..=lookahead {
                if ni + delta < new.len() && oi < old.len() && old[oi] == new[ni + delta] {
                    for line in &new[ni..ni + delta] {
                        out.push(format!("+ {line}"));
                    }
                    ni += delta;
                    matched = true;
                    break;
                }
                if oi + delta < old.len() && ni < new.len() && old[oi + delta] == new[ni] {
                    for line in &old[oi..oi + delta] {
                        out.push(format!("- {line}"));
                    }
                    oi += delta;
                    matched = true;
                    break;
                }
            }
            if !matched {
                if oi < old.len() {
                    out.push(format!("- {}", old[oi]));
                    oi += 1;
                }
                if ni < new.len() {
                    out.push(format!("+ {}", new[ni]));
                    ni += 1;
                }
            }
        }
    }

    out
}
