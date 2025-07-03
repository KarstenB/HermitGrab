use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use clap::{Parser, Subcommand};
use git2::Repository;

use crate::{
    LinkType, RequireTag,
    config::{FallbackOperation, GlobalConfig, Tag},
    detector,
};
use crate::{hermitgrab_info, info};

pub mod cmd_add;
pub mod cmd_apply;
#[cfg(feature = "interactive")]
pub mod cmd_apply_tui;
pub mod cmd_init;
pub mod cmd_status;

fn long_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        format!(
            "{} (commit: {} epoch: {})",
            env!("CARGO_PKG_VERSION"),
            option_env!("CARGO_MAKE_GIT_HEAD_LAST_COMMIT_HASH_PREFIX").unwrap_or("<unknown>"),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
    })
}

#[derive(Parser)]
#[command(name = "hermitgrab")]
#[command(version, long_version = long_version())]
#[command(about = "A modern dotfile manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// Run in interactive TUI mode
    #[arg(short = 'i', env = "HERMIT_INTERACTIVE", global = true)]
    #[cfg(feature = "interactive")]
    pub interactive: bool,
    /// Increase output verbosity
    #[arg(short = 'v', long, env = "HERMIT_VERBOSE", global = true)]
    pub verbose: bool,
    #[arg(short = 'y', long, env = "HERMIT_CONFIRM", global = true)]
    pub confirm: bool,
    /// Path to the hermitgrab config directory
    /// If not set, defaults to ~/.hermitgrab
    #[arg(
        short = 'c',
        long,
        env = "HERMIT_DIR",
        global = true,
        value_name = "PATH"
    )]
    pub hermit_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum AddCommand {
    Config {
        /// Subdirectory of the hermit.toml file to add the target directory to
        config_dir: PathBuf,
        /// Tags this config provides
        #[arg(short = 'p', long = "provides", value_name = "TAG", num_args = 1..)]
        tags: Vec<Tag>,
        /// Tags this config requires for all of its links and other actions
        #[arg(short = 'r', long = "requires", value_name = "TAG", num_args = 0..)]
        required_tags: Vec<RequireTag>,
    },
    /// Add a new Link to the config
    Link {
        /// Subdirectory of the hermit.toml file to add the link to
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Source file or directory to link
        source: PathBuf,
        /// Link type to use
        #[arg(short = 'l', long, default_value = "soft", value_enum)]
        link_type: LinkType,
        /// Destination path for the link, if not specified, uses the source name
        #[arg(short = 'd', long)]
        destination: Option<String>,
        /// Required tags to include in the link (can be specified multiple times).
        /// A tag can start with a + to indicate it is required or a - to indicate it has to be excluded when present.
        #[arg(short = 't', long = "tag", value_name = "TAG", num_args = 0..)]
        required_tags: Vec<RequireTag>,
        /// Provided tags in case a new config file will be created, i.e. destination does not yet exist.
        #[arg(short = 'p', long = "provides", value_name = "TAG", num_args = 0..)]
        provided_tags: Vec<Tag>,
        /// Fallback strategy in case the destination already exists
        #[arg(short = 'f', long, default_value = "abort", value_enum)]
        fallback: FallbackOperation,
    },
    /// Add a new profile to the config
    Profile {
        /// Name of the profile to add
        name: String,
        /// Tags to include in the profile (can be specified multiple times)
        #[arg(short = 't', long = "tag", value_name = "TAG", num_args = 0..)]
        tags: Vec<Tag>,
    },
}
#[derive(Subcommand)]
pub enum GetCommand {
    /// Show all tags (including auto detected)
    Tags,
    /// Show all profiles (from all configs)
    Profiles,
}

#[derive(Subcommand)]
pub enum Provider {
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
pub enum InitCommand {
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
pub enum Commands {
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
        /// Arguments to pass to UBI (The Universal Binary Installer)
        #[arg(last = true)]
        ubi_args: Vec<String>,
    },
    Add {
        #[command(subcommand)]
        add_command: AddCommand,
    },
}

pub async fn execute(
    command: Commands,
    global_config: Arc<GlobalConfig>,
    confirm: bool,
    verbose: bool,
    interactive: bool,
) -> Result<(), anyhow::Error> {
    let search_root = global_config.hermit_dir();
    match command {
        Commands::Init { init_command } => match init_command {
            InitCommand::Clone { repo } => {
                let pat = std::env::var("HERMITGRAB_GITHUB_TOKEN");
                cmd_init::clone_or_update_repo(&repo, pat.ok().as_deref(), &global_config)?;
            }
            InitCommand::Discover { create, provider } => {
                if search_root.exists() {
                    info!(
                        "Dotfiles directory already exists at {}",
                        search_root.display()
                    );
                    Repository::open(search_root)?;
                    info!("Repository already initialized, skipping discovery.");
                    return Ok(());
                }
                match provider {
                    Provider::GitHub { token } => {
                        cmd_init::discover_repo_with_github(create, token, &global_config).await?;
                    }
                    Provider::GitLab { token } => {
                        cmd_init::discover_repo_with_gitlab(create, token).await?;
                    }
                    Provider::AzureDevOps { token } => {
                        cmd_init::discover_repo_with_azure_devops(create, token).await?;
                    }
                }
            }
            InitCommand::Create => {
                cmd_init::create_local_repo(&global_config)?;
            }
        },
        Commands::Add { add_command } => match add_command {
            AddCommand::Config {
                ref config_dir,
                ref tags,
                ref required_tags,
            } => {
                cmd_add::add_config(config_dir, tags, required_tags, &[], &[], &global_config)?;
            }
            AddCommand::Link {
                ref config_dir,
                ref source,
                ref link_type,
                ref destination,
                ref required_tags,
                ref provided_tags,
                ref fallback,
            } => {
                cmd_add::add_link(
                    config_dir,
                    source,
                    link_type,
                    destination,
                    required_tags,
                    provided_tags,
                    fallback,
                    &global_config,
                )?;
            }
            AddCommand::Profile { ref name, ref tags } => {
                cmd_add::add_profile(name, tags, &global_config)?;
            }
        },
        Commands::Apply {
            ref tags,
            ref profile,
        } => {
            #[cfg(feature = "interactive")]
            if interactive {
                cmd_apply_tui::run_tui(&global_config, tags, profile)?;
            } else {
                cmd_apply::apply_with_tags(&global_config, confirm, verbose, tags, profile)?;
            }
            #[cfg(not(feature = "interactive"))]
            {
                let _ = interactive;
                cmd_apply::apply_with_tags(&global_config, confirm, verbose, tags, profile)?;
            }
        }
        Commands::Status => {
            cmd_status::get_status(&global_config)?;
        }
        Commands::Get { get_command } => match get_command {
            GetCommand::Tags => {
                let mut all_tags = global_config.all_provided_tags().clone();
                let detected_tags = detector::detect_builtin_tags();
                all_tags.extend(detected_tags);
                hermitgrab_info("All tags (including auto-detected):");
                for t in all_tags {
                    info!("- {} ({})", t.name(), t.source());
                }
            }
            GetCommand::Profiles => {
                hermitgrab_info("All profiles:");
                for (profile, tags) in global_config.all_profiles() {
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
            use crate::integrations;

            ubi_args.insert(0, "hermitgrab ubi --".to_string());
            hermitgrab_info!("Running UBI with args: {:?}", ubi_args);
            integrations::ubi_int::main(&ubi_args).await
        }
    }
    Ok(())
}
