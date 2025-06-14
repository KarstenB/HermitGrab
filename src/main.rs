use anyhow::Result;
use clap::{Parser, Subcommand};
use directories::UserDirs;
use git2::Repository;

pub mod action;
pub mod cmd_apply;
#[cfg(feature = "interactive")]
pub mod cmd_apply_tui;
pub mod cmd_init;
pub mod common_cli;
pub mod config;
pub mod detector;
pub mod execution_plan;
pub mod hermitgrab_error;
pub mod links_files;
#[cfg(feature = "ubi")]
pub mod ubi_int;

pub use crate::action::{Action, InstallAction, LinkAction};
use crate::common_cli::{hermitgrab_info, info};
use crate::config::find_hermit_yaml_files;
pub use crate::config::{DotfileEntry, HermitConfig, InstallEntry, LinkType, RequireTag};
pub use crate::hermitgrab_error::AtomicLinkError;
pub use std::collections::HashSet;
use std::path::PathBuf;
pub use std::sync::Arc;
use std::sync::OnceLock;

#[derive(Parser)]
#[command(name = "hermitgrab")]
#[command(about = "A modern dotfile manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Run in interactive TUI mode
    #[arg(short = 'i', env = "HERMIT_INTERACTIVE", global = true)]
    #[cfg(feature = "interactive")]
    interactive: bool,
    /// Increase output verbosity
    #[arg(short = 'v', long, env = "HERMIT_VERBOSE", global = true)]
    verbose: bool,
    #[arg(short = 'y', long, env = "HERMIT_CONFIRM", global = true)]
    confirm: bool,
    /// Path to the hermitgrab config directory
    /// If not set, defaults to ~/.hermitgrab
    #[arg(
        short = 'c',
        long,
        env = "HERMIT_DIR",
        global = true,
        value_name = "PATH"
    )]
    hermit_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum GetCommand {
    /// Show all tags (including auto detected)
    Tags,
    /// Show all profiles (from all configs)
    Profiles,
}

#[derive(Subcommand)]
enum Provider {
    /// Use GitHub as the provider
    GitHub {
        #[arg(long, env = "HERMIT_GITHUB_TOKEN")]
        token: Option<String>,
    },
    /// Use GitLab as the provider
    GitLab {
        #[arg(long, env = "HERMIT_GITLAB_TOKEN")]
        token: Option<String>,
    },
    /// Use AzureDevOps as the provider
    AzureDevOps {
        #[arg(long, env = "HERMIT_AZURE_DEVOPS_TOKEN")]
        token: Option<String>,
    },
}

#[derive(Subcommand)]
enum InitCommand {
    /// Clone a dotfiles repo from a given URL
    Clone {
        /// Git repository URL
        repo: String,
    },
    /// Discover dotfiles repo on GitHub, GitLab or AzureDevOps
    Discover {
        /// Create the repo if not found
        #[arg(long)]
        create: bool,
        /// Provider to use for discovery
        #[command(subcommand)]
        provider: Provider,
    },
    /// Create an empty local dotfiles repo
    Create,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage dotfiles repo initialization
    Init {
        #[command(subcommand)]
        init_command: InitCommand,
    },
    /// Install applications and link/copy dotfiles
    Apply {
        /// Include actions matching these tags (can be specified multiple times)
        #[arg(short='t', long = "tag", env="HERMIT_TAGS", value_name = "TAG", num_args = 0.., global = true)]
        tags: Vec<String>,
        /// Use a named profile which is a set of tags
        #[arg(
            short = 'p',
            long,
            env = "HERMIT_PROFILE",
            value_name = "PROFILE",
            global = true
        )]
        profile: Option<String>,
    },
    /// Show status of managed files
    Status,
    /// Show tags or profiles
    Get {
        #[command(subcommand)]
        get_command: GetCommand,
    },
    #[cfg(feature = "ubi")]
    /// Run UBI for installing applications
    Ubi {
        /// Arguments to pass to UBI
        #[arg(last = true)]
        ubi_args: Vec<String>,
    },
}

static SEARCH_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn init_hermit_dir(cli_path: &Option<PathBuf>) -> std::path::PathBuf {
    if let Some(path) = cli_path {
        hermitgrab_info!("Using hermit directory from CLI: {}", path.display());
        return path.clone();
    }
    let user_dirs = UserDirs::new().expect("Could not get user directories");
    let dotfiles_dir = user_dirs.home_dir().join(".hermitgrab");
    hermitgrab_info!(
        "Using hermit directory from user dirs: {}",
        dotfiles_dir.display()
    );
    dotfiles_dir
}
pub fn hermit_dir() -> PathBuf {
    SEARCH_ROOT
        .get()
        .expect("Hermit directory not set")
        .to_path_buf()
}

#[tokio::main]
async fn main() -> Result<()> {
    // simple_logger::init_with_env().unwrap();
    let cli = Cli::parse();
    let search_root = SEARCH_ROOT.get_or_init(|| init_hermit_dir(&cli.hermit_dir));
    let yaml_files = find_hermit_yaml_files(search_root);
    let global_config = config::GlobalConfig::from_paths(search_root, &yaml_files)?;
    match cli.command {
        Commands::Init { init_command } => match init_command {
            InitCommand::Clone { repo } => {
                let pat = std::env::var("HERMITGRAB_GITHUB_TOKEN");
                crate::cmd_init::clone_or_update_repo(repo, pat.ok().as_deref())?;
            }
            InitCommand::Discover { create, provider } => {
                let hermit_dir = hermit_dir();
                if hermit_dir.exists() {
                    info!(
                        "Dotfiles directory already exists at {}",
                        hermit_dir.display()
                    );
                    Repository::open(&hermit_dir)?;
                    info!("Repository already initialized, skipping discovery.");
                    return Ok(());
                }
                match provider {
                    Provider::GitHub { token } => {
                        crate::cmd_init::discover_repo_with_github(create, token).await?;
                    }
                    Provider::GitLab { token } => {
                        crate::cmd_init::discover_repo_with_gitlab(create, token).await?;
                    }
                    Provider::AzureDevOps { token } => {
                        crate::cmd_init::discover_repo_with_azure_devops(create, token).await?;
                    }
                }
            }
            InitCommand::Create => {
                crate::cmd_init::create_local_repo()?;
            }
        },
        Commands::Apply {
            ref tags,
            ref profile,
        } => {
            #[cfg(feature = "interactive")]
            if cli.interactive {
                cmd_apply_tui::run_tui(&global_config, &cli, tags, profile)?;
            } else {
                cmd_apply::apply_with_tags(&global_config, &cli, tags, profile)?;
            }
            #[cfg(not(feature = "interactive"))]
            {
                cmd_apply::apply_with_tags(&global_config, &cli, tags, profile)?;
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
        #[cfg(feature = "ubi")]
        Commands::Ubi { mut ubi_args } => {
            ubi_args.insert(0, "hermitgrab ubi --".to_string());
            hermitgrab_info!("Running UBI with args: {:?}", ubi_args);
            ubi_int::main(&ubi_args).await
        }
    }
    Ok(())
}
