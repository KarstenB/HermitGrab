use crate::LinkType;
use anyhow::Result;
use handlebars::Handlebars;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

fn color_enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn colorize(msg: &str, color: &str) -> String {
    if color_enabled() {
        match color {
            "red" => format!("\x1b[31m{}\x1b[0m", msg),
            "green" => format!("\x1b[32m{}\x1b[0m", msg),
            "yellow" => format!("\x1b[33m{}\x1b[0m", msg),
            "blue" => format!("\x1b[34m{}\x1b[0m", msg),
            "magenta" => format!("\x1b[35m{}\x1b[0m", msg),
            "cyan" => format!("\x1b[36m{}\x1b[0m", msg),
            "light_gray" => format!("\x1b[90m{}\x1b[0m", msg),
            "light_blue" => format!("\x1b[94m{}\x1b[0m", msg),
            _ => msg.to_string(),
        }
    } else {
        msg.to_string()
    }
}

fn info_prefix() -> String {
    colorize("[hermitgrab]", "light_blue")
}

pub fn find_hermit_yaml_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == "hermit.yaml") {
        result.push(root.to_path_buf());
    } else if root.is_dir() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(find_hermit_yaml_files(&path));
                } else if path.file_name().is_some_and(|f| f == "hermit.yaml") {
                    result.push(path);
                }
            }
        }
    }
    result
}

pub fn run_with_dir(config_dir: Option<&str>, verbose: bool) -> Result<()> {
    use directories::UserDirs;
    println!("{} Applying configuration...", info_prefix());
    let search_root = if let Some(dir) = config_dir {
        PathBuf::from(dir)
    } else {
        let user_dirs = UserDirs::new().expect("Could not get user directories");
        user_dirs.home_dir().join(".hermitgrab")
    };
    let yaml_files = find_hermit_yaml_files(&search_root);
    if yaml_files.is_empty() {
        eprintln!(
            "{} No hermit.yaml files found in {}",
            info_prefix(),
            search_root.display()
        );
        return Ok(());
    }
    let hermitgrab_root = search_root.canonicalize().unwrap_or(search_root.clone());
    let mut total_errors = 0;
    for config_path in yaml_files {
        let rel_path = config_path
            .strip_prefix(&hermitgrab_root)
            .unwrap_or(&config_path);
        println!(
            "{} Processing config: {}",
            info_prefix(),
            rel_path.display()
        );
        let config = match crate::load_hermit_config(config_path.to_str().unwrap()) {
            Ok(cfg) => cfg,
            Err(_) => {
                eprintln!(
                    "[hermitgrab] Error loading {}: (could not parse or read)",
                    rel_path.display()
                );
                total_errors += 1;
                continue;
            }
        };
        let mut errors = Vec::new();
        let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        for entry in config.files {
            let src = config_dir.join(&entry.source);
            let dst_path_str = shellexpand::tilde(&entry.target).to_string();
            let dst = std::path::Path::new(&dst_path_str);
            // Ensure the parent directory of the destination exists
            if let Some(parent) = dst.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        let msg = format!(
                            "[hermitgrab] Error creating directory {} for target {} (from {}): {}",
                            parent.display(),
                            dst.display(),
                            config_path.display(),
                            e
                        );
                        let colored = colorize(&msg, "red");
                        if verbose {
                            eprintln!("{}", colored);
                        }
                        errors.push(colored);
                        continue;
                    }
                }
            }
            if verbose {
                println!(
                    "{} {} -> {} ({:?})",
                    info_prefix(),
                    src.display(),
                    entry.target,
                    entry.link
                );
            }
            match entry.link {
                LinkType::Soft => match crate::atomic_link::atomic_symlink(&src, dst) {
                    Ok(_) => {
                        if verbose {
                            println!(
                                "{} Symlinked {} -> {}",
                                info_prefix(),
                                src.display(),
                                dst.display()
                            );
                        }
                    }
                    Err(e) => {
                        let msg = format!(
                            "[hermitgrab] Error creating symlink for {} -> {} (from {}): {}",
                            src.display(),
                            entry.target,
                            config_path.display(),
                            e
                        );
                        let colored = colorize(&msg, "red");
                        if verbose {
                            eprintln!("{}", colored);
                        }
                        errors.push(colored);
                        continue;
                    }
                },
                LinkType::Copy => {
                    if let Err(e) = std::fs::copy(&src, dst) {
                        let msg = format!(
                            "[hermitgrab] Error copying {} -> {} (from {}): {}",
                            src.display(),
                            entry.target,
                            config_path.display(),
                            e
                        );
                        let colored = colorize(&msg, "red");
                        if verbose {
                            eprintln!("{}", colored);
                        }
                        errors.push(colored);
                        continue;
                    }
                    if verbose {
                        println!(
                            "{} Copied {} -> {}",
                            info_prefix(),
                            src.display(),
                            dst.display()
                        );
                    }
                }
                LinkType::Hard => {
                    if let Err(e) = std::fs::hard_link(&src, dst) {
                        let msg = format!(
                            "[hermitgrab] Error hard linking {} -> {} (from {}): {}",
                            src.display(),
                            entry.target,
                            config_path.display(),
                            e
                        );
                        let colored = colorize(&msg, "red");
                        if verbose {
                            eprintln!("{}", colored);
                        }
                        errors.push(colored);
                        continue;
                    }
                    if verbose {
                        println!(
                            "{} Hard linked {} -> {}",
                            info_prefix(),
                            src.display(),
                            dst.display()
                        );
                    }
                }
            }
        }
        let reg = Handlebars::new();
        for entry in config.install {
            let name = entry.name;
            let check_cmd = entry.check_cmd.as_deref().unwrap_or("");
            let source = entry.source.as_deref().unwrap_or("");
            if !check_cmd.is_empty() {
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(check_cmd)
                    .output();
                if let Ok(output) = status {
                    if output.status.success() {
                        let msg = format!(
                            "{} {} already installed (check: '{}')",
                            info_prefix(),
                            name,
                            check_cmd
                        );
                        println!("{}", colorize(&msg, "yellow"));
                        if verbose {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            if !stdout.trim().is_empty() {
                                println!(
                                    "{}",
                                    colorize(
                                        &format!("[check_cmd stdout] {}", stdout.trim()),
                                        "light_gray"
                                    )
                                );
                            }
                            if !stderr.trim().is_empty() {
                                println!(
                                    "{}",
                                    colorize(
                                        &format!("[check_cmd stderr] {}", stderr.trim()),
                                        "light_gray"
                                    )
                                );
                            }
                        }
                        continue;
                    }
                }
            }
            if let Some(template) = config.sources.get(source) {
                let cmd = reg
                    .render_template(template, &entry.check_cmd)
                    .unwrap_or_else(|_| template.clone());
                println!(
                    "[hermitgrab] Installing {} using '{}': {}",
                    name, source, cmd
                );
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .status();
                if let Ok(status) = status {
                    if status.success() {
                        println!("[hermitgrab] Successfully installed {}", name);
                    } else {
                        println!(
                            "[hermitgrab] Failed to install {} (exit code: {:?})",
                            name,
                            status.code()
                        );
                    }
                } else {
                    println!("[hermitgrab] Failed to run install command for {}", name);
                }
            } else {
                println!("[hermitgrab] Unknown source: {} for {}", source, name);
            }
        }
        if !errors.is_empty() {
            let summary = colorize(
                &format!(
                    "{} {} error(s) occurred in {}. Use --verbose for details.",
                    info_prefix(),
                    errors.len(),
                    rel_path.display()
                ),
                "red",
            );
            eprintln!("{}", summary);
            if verbose {
                for err in &errors {
                    eprintln!("{}", err);
                }
            }
            total_errors += errors.len();
        } else {
            println!(
                "{} All operations completed successfully for {}.",
                info_prefix(),
                rel_path.display()
            );
        }
    }
    if total_errors > 0 {
        eprintln!(
            "{} Total errors across all configs: {}",
            info_prefix(),
            total_errors
        );
    }
    Ok(())
}

pub fn run() -> Result<()> {
    run_with_dir(None, false)
}
