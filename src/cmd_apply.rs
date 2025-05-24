use crate::LinkType;
use anyhow::Result;
use handlebars::Handlebars;

pub fn run_with_dir(config_dir: Option<&str>) -> Result<()> {
    use directories::UserDirs;
    println!("[hermitgrab] Applying configuration...");
    let config_path = if let Some(dir) = config_dir {
        std::path::PathBuf::from(dir).join("hermit.yaml")
    } else {
        let user_dirs = UserDirs::new().expect("Could not get user directories");
        user_dirs.home_dir().join(".hermitgrab/hermit.yaml")
    };
    let config = match crate::load_hermit_config(config_path.to_str().unwrap()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "[hermitgrab] Error loading {}: \n---\n{}\n---",
                config_path.display(),
                std::fs::read_to_string(&config_path)
                    .unwrap_or_else(|_| format!("(could not read {})", config_path.display()))
            );
            return Err(e);
        }
    };
    let config_dir = config_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    for entry in config.files {
        let src = config_dir.join(&entry.source);
        let dst_path_str = shellexpand::tilde(&entry.target).to_string();
        let dst = std::path::Path::new(&dst_path_str);
        // Ensure the parent directory of the destination exists
        if let Some(parent) = dst.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!(
                        "[hermitgrab] Error creating directory {} for target {} (from {}): {}",
                        parent.display(), dst.display(), config_path.display(), e
                    );
                    continue;
                }
            }
        }
        println!(
            "[hermitgrab] {} -> {} ({:?})",
            src.display(), entry.target, entry.link
        );
        match entry.link {
            LinkType::Soft => {
                match crate::atomic_link::atomic_symlink(&src, dst) {
                    Ok(_) => println!(
                        "[hermitgrab] Symlinked {} -> {}",
                        src.display(),
                        dst.display()
                    ),
                    Err(e) => {
                        eprintln!(
                            "[hermitgrab] Error creating symlink for {} -> {} (from {}): {}",
                            src.display(), entry.target, config_path.display(), e
                        );
                        continue;
                    }
                }
            }
            LinkType::Copy => {
                if let Err(e) = std::fs::copy(&src, dst) {
                    eprintln!(
                        "[hermitgrab] Error copying {} -> {} (from {}): {}",
                        src.display(), entry.target, config_path.display(), e
                    );
                    continue;
                }
                println!("[hermitgrab] Copied {} -> {}", src.display(), dst.display());
            }
            LinkType::Hard => {
                if let Err(e) = std::fs::hard_link(&src, dst) {
                    eprintln!(
                        "[hermitgrab] Error hard linking {} -> {} (from {}): {}",
                        src.display(), entry.target, config_path.display(), e
                    );
                    continue;
                }
                println!(
                    "[hermitgrab] Hard linked {} -> {}",
                    src.display(),
                    dst.display()
                );
            }
        }
    }
    if let Some(apps) = config.install {
        if let Some(sources) = &config.sources {
            let reg = Handlebars::new();
            for entry in apps {
                let name = entry.name().unwrap_or("");
                let check_cmd = entry.check_cmd().unwrap_or("");
                let source = entry.source().unwrap_or("");
                if !check_cmd.is_empty() {
                    let status = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(check_cmd)
                        .status();
                    if let Ok(status) = status {
                        if status.success() {
                            println!(
                                "[hermitgrab] {} already installed (check: '{}')",
                                name, check_cmd
                            );
                            continue;
                        }
                    }
                }
                if let Some(template) = sources.get(source) {
                    let cmd = reg
                        .render_template(template, &entry.0)
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
        }
    }
    Ok(())
}

pub fn run() -> Result<()> {
    run_with_dir(None)
}
