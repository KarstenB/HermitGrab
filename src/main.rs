use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod action;
pub mod atomic_link;
pub mod cmd_apply;
pub mod cmd_apply_tui;
pub mod cmd_init;
pub mod config;
pub mod detector;
pub mod execution_plan;
pub mod hermitgrab_error;

pub use crate::action::{Action, AtomicLinkAction, InstallAction};
pub use crate::cmd_init::run as init_command;
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
    #[arg(long = "interactive", global = true)]
    interactive: bool,
    /// Increase output verbosity
    #[arg(short, long, global = true)]
    verbose: bool,
    #[arg(short = 'y', long, global = true)]
    confirm: bool,
    /// Only include actions matching these tags (can be specified multiple times)
    #[arg(long = "tag", value_name = "TAG", num_args = 0.., global = true)]
    tags: Vec<String>,
    /// Use a named profile (collects tags from all configs)
    #[arg(long = "profile", value_name = "PROFILE", global = true)]
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
    let yaml_files = crate::cmd_apply::find_hermit_yaml_files(&search_root);
    let global_config = config::GlobalConfig::from_paths(search_root,&yaml_files)?;
    match cli.command {
        Commands::Init { repo } => {
            crate::cmd_init::run(repo)?;
        }
        Commands::Apply => {
            if cli.interactive {
                // Only pass config and cli to TUI, let it compute active_tags and actions internally
                return cmd_apply_tui::run_tui(&global_config, &cli)
                    .map_err(|e| anyhow::anyhow!(e));
            } else {
                cmd_apply::apply_with_tags(cli, &global_config)?;
            }
        }
        Commands::Status => {
            println!("[hermitgrab] Status:");
            // TODO: Implement status reporting
        }
        Commands::Get { get_command } => match get_command {
            GetCommand::Tags => {
                let mut all_tags = global_config.all_tags.clone();
                let detected_tags = detector::detect_builtin_tags();
                all_tags.extend(detected_tags);
                println!("All tags (including auto-detected):");
                for t in all_tags {
                    println!("- {}", t);
                }
            }
            GetCommand::Profiles => {
                println!("All profiles:");
                for (profile, tags) in &global_config.all_profiles {
                    println!(
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
