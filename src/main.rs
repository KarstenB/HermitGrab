use crate::commands::{Cli, Commands};
use crate::config::{GlobalConfig, find_hermit_files};
pub use crate::config::{HermitConfig, InstallConfig, LinkConfig, LinkType, RequireTag};
pub use crate::hermitgrab_error::FileOpsError;
use crate::{
    common_cli::{hermitgrab_info, info},
    config::CONF_FILE_NAME,
};
use anyhow::Result;
use clap::Parser;
use directories::UserDirs;
pub use std::collections::HashSet;
use std::path::PathBuf;
pub use std::sync::Arc;

pub mod action;
pub mod commands;
pub mod common_cli;
pub mod config;
pub mod detector;
pub mod execution_plan;
pub mod file_ops;
pub mod hermitgrab_error;
pub mod integrations;

fn init_hermit_dir(cli_path: &Option<PathBuf>) -> std::path::PathBuf {
    if let Some(path) = cli_path {
        hermitgrab_info!("Using hermit directory from CLI: {}", path.display());
        return path.clone();
    }
    let user_dirs = UserDirs::new().expect("Could not get user directories");
    let dotfiles_dir = user_dirs.home_dir().join(".hermitgrab");
    if !dotfiles_dir.exists() {
        let path_buf = std::env::current_exe().ok();
        if let Some(exe) = path_buf {
            let exe_dir = exe.parent();
            if let Some(exe_dir) = exe_dir {
                if exe_dir.join(CONF_FILE_NAME).exists() {
                    hermitgrab_info!(
                        "Using hermit directory beside executable {}",
                        dotfiles_dir.display()
                    );
                    return exe_dir.to_path_buf();
                }
            }
        }
    }
    hermitgrab_info!(
        "Using hermit directory from user dirs: {}",
        dotfiles_dir.display()
    );
    dotfiles_dir
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command;
    if !matches!(command, Commands::Ubi { .. }) {
        simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Error)
            .env()
            .init()?;
    }
    let search_root = init_hermit_dir(&cli.hermit_dir);
    let yaml_files = find_hermit_files(&search_root);
    let home_dir = UserDirs::new()
        .expect("Could not get user directories")
        .home_dir()
        .to_path_buf();
    let global_config = GlobalConfig::from_paths(&search_root, &home_dir, &yaml_files)?;
    let interactive = false;
    #[cfg(feature = "interactive")]
    let interactive = cli.interactive;
    commands::execute(
        command,
        global_config,
        cli.confirm,
        cli.verbose,
        interactive,
        cli.json,
    )
    .await?;

    Ok(())
}
