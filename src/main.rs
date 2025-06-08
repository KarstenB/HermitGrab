use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod action;
pub mod cmd_apply;
pub mod cmd_apply_tui;
pub mod cmd_init;
pub mod common_cli;
pub mod config;
pub mod detector;
pub mod execution_plan;
pub mod hermitgrab_error;
pub mod links_files;

pub use crate::action::{Action, InstallAction, LinkAction};
pub use crate::cmd_init::run as init_command;
use crate::common_cli::{hermitgrab_info, info};
use crate::config::find_hermit_yaml_files;
pub use crate::config::{DotfileEntry, HermitConfig, InstallEntry, LinkType, RequireTag};
pub use crate::hermitgrab_error::AtomicLinkError;
pub use std::collections::HashSet;
pub use std::sync::Arc;

#[derive(Parser)]
#[command(name = "hermitgrab")]
#[command(about = "A modern dotfile manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Run in interactive TUI mode
    #[arg(short = 'i', long, global = true)]
    interactive: bool,
    /// Increase output verbosity
    #[arg(short = 'v', long, global = true)]
    verbose: bool,
    #[arg(short = 'y', long, global = true)]
    confirm: bool,
    /// Include actions matching these tags (can be specified multiple times)
    #[arg(short='t', long = "tag", value_name = "TAG", num_args = 0.., global = true)]
    tags: Vec<String>,
    /// Use a named profile which is a set of tags
    #[arg(short = 'p', long, value_name = "PROFILE", global = true)]
    profile: Option<String>,
}

#[derive(Subcommand)]
enum GetCommand {
    /// Show all tags (including auto detected)
    Tags,
    /// Show all profiles (from all configs)
    Profiles,
}

#[derive(Subcommand)]
enum Commands {
    /// Clone a dotfiles repo from GitHub
    Init {
        /// GitHub repository URL
        repo: String,
    },
    /// Install applications and link/copy dotfiles
    Apply,
    /// Show status of managed files
    Status,
    /// Show tags or profiles
    Get {
        #[command(subcommand)]
        get_command: GetCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let user_dirs = directories::UserDirs::new().expect("Could not get user directories");
    let search_root = user_dirs.home_dir().join(".hermitgrab");
    let yaml_files = find_hermit_yaml_files(&search_root);
    let global_config = config::GlobalConfig::from_paths(search_root, &yaml_files)?;
    match cli.command {
        Commands::Init { repo } => {
            crate::cmd_init::run(repo)?;
        }
        Commands::Apply => {
            if cli.interactive {
                cmd_apply_tui::run_tui(&global_config, &cli)?;
            } else {
                cmd_apply::apply_with_tags(cli, &global_config)?;
            }
        }
        Commands::Status => {
            hermitgrab_info("Status:");
            // TODO: Implement status reporting
        }
        Commands::Get { get_command } => match get_command {
            GetCommand::Tags => {
                let mut all_tags = global_config.all_provided_tags.clone();
                let detected_tags = detector::detect_builtin_tags();
                all_tags.extend(detected_tags);
                hermitgrab_info("All tags (including auto-detected):");
                for t in all_tags {
                    info!("- {} ({})", t.name(), t.source());
                }
            }
            GetCommand::Profiles => {
                hermitgrab_info("All profiles:");
                for (profile, tags) in &global_config.all_profiles {
                    info!(
                        "- {}: {}",
                        profile,
                        tags.iter()
                            .map(|t| t.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        },
    }
    Ok(())
}
