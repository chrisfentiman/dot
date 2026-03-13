use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::dotfiles;

enum ConfigStatus {
    Ok,
    MissingSymlink,
    BrokenSymlink,
    MissingTemplate,
    WrongTarget(String),
}

pub fn run() -> Result<()> {
    let symlinks = dotfiles::read_symlinks()?;

    if symlinks.symlinks.is_empty() {
        println!(
            "No managed configs. Run {} to add one.",
            "dotf config <path>".cyan()
        );
        return Ok(());
    }

    let configs_dir = dotfiles::configs_dir()?;

    let mut entries: Vec<_> = symlinks.symlinks.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    let name_width = entries
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let target_width = entries
        .iter()
        .map(|(_, v)| v.len())
        .max()
        .unwrap_or(6)
        .max(6);

    println!(
        "{:<name_width$}  {:<target_width$}  {}",
        "CONFIG".bold(),
        "TARGET".bold(),
        "STATUS".bold()
    );
    println!("{}", "─".repeat(name_width + target_width + 10).dimmed());

    for (name, target_str) in &entries {
        let template_path = configs_dir.join(format!("{name}.tmpl"));
        let output_path = configs_dir.join(name);

        let status = if !template_path.exists() {
            ConfigStatus::MissingTemplate
        } else {
            match dotfiles::expand_tilde(target_str) {
                Err(_) => ConfigStatus::MissingSymlink,
                Ok(link_path) => {
                    if link_path.symlink_metadata().is_err() {
                        ConfigStatus::MissingSymlink
                    } else {
                        match fs::read_link(&link_path) {
                            Err(_) => ConfigStatus::BrokenSymlink,
                            Ok(dest) => {
                                if dest == output_path {
                                    ConfigStatus::Ok
                                } else {
                                    ConfigStatus::WrongTarget(dest.display().to_string())
                                }
                            }
                        }
                    }
                }
            }
        };

        let status_str = match &status {
            ConfigStatus::Ok => "ok".green().bold().to_string(),
            ConfigStatus::MissingSymlink => "missing symlink".yellow().bold().to_string(),
            ConfigStatus::BrokenSymlink => "broken symlink".red().bold().to_string(),
            ConfigStatus::MissingTemplate => "missing template".red().bold().to_string(),
            ConfigStatus::WrongTarget(t) => format!("wrong target: {}", t.red()),
        };

        println!(
            "{:<name_width$}  {:<target_width$}  {}",
            name.cyan(),
            target_str,
            status_str
        );
    }

    Ok(())
}
