// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use clap::{Parser, Subcommand};
use git2::Repository;

use crate::{
    LinkType, RequireTag,
    config::{CliOptions, FallbackOperation, GlobalConfig, PatchType, Tag},
    detector,
};
use crate::{hermitgrab_info, info};

pub mod cmd_add;
pub mod cmd_apply;
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
    /// Don't ask for confirmation, assume yes is the answer
    #[arg(short = 'y', long, env = "HERMIT_CONFIRM", global = true)]
    pub confirm: bool,
    /// Path to the hermitgrab config directory
    /// If not set, defaults to ~/.hermitgrab
    #[arg(
        short = 'c',
        long,
        env = "HERMIT_DIR",
        global = true,
        value_name = "PATH",
        value_hint = clap::ValueHint::DirPath,
    )]
    pub hermit_dir: Option<PathBuf>,
    #[arg(
        long,
        env = "HERMIT_JSON",
        global = true,
        value_name = "PATH",
        value_hint = clap::ValueHint::FilePath,
        hide = true,
    )]
    pub json: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum AddCommand {
    Config {
        /// Subdirectory of the hermit.toml file to add the target directory to
        config_dir: PathBuf,
        /// Tags this config requires for all of its links and other actions
        #[arg(short = 'r', long = "requires", value_name = "TAG", num_args = 0..)]
        required_tags: Vec<RequireTag>,
    },
    /// Add a new Link to the config
    Link {
        /// Source file or directory to link
        #[arg(value_hint = clap::ValueHint::FilePath)]
        source: PathBuf,
        /// Subdirectory of the hermit.toml file to add the link to
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Link type to use
        #[arg(short = 'l', long, default_value = "soft", value_enum)]
        link_type: LinkType,
        /// Target path for the link, if not specified, uses the source name
        #[arg(short = 't', long)]
        target: Option<PathBuf>,
        /// Required tags to include in the link (can be specified multiple times).
        /// A tag can start with a + to indicate it is required or a - to indicate it has to be excluded when present.
        #[arg(short = 'r', long = "requires", value_name = "TAG", num_args = 0..)]
        required_tags: Vec<RequireTag>,
        /// Fallback strategy in case the target already exists
        #[arg(short = 'f', long, default_value = "abort", value_enum)]
        fallback: FallbackOperation,
    },
    /// Add a new Link to the config
    Patch {
        /// Source file to patch
        #[arg(value_hint = clap::ValueHint::FilePath)]
        source: PathBuf,
        /// Subdirectory of the hermit.toml file to add the Patch to
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Patch type to use
        #[arg(short = 'p', long, default_value = "JsonMerge", value_enum)]
        patch_type: PatchType,
        /// Target path for the patch, if not specified, uses the source name
        #[arg(short = 't', long, value_hint = clap::ValueHint::FilePath)]
        target: Option<PathBuf>,
        /// Required tags to include in the link (can be specified multiple times).
        /// A tag can start with a + to indicate it is required or a - to indicate it has to be excluded when present.
        #[arg(short = 'r', long = "requires", value_name = "TAG", num_args = 0..)]
        required_tags: Vec<RequireTag>,
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
    /// Config
    Config,
}

#[derive(Subcommand)]
pub enum Provider {
    /// Use GitHub as the provider
    GitHub {
        /// A Github Token to use instead of the device authentication
        #[arg(long, env = "HERMIT_GITHUB_TOKEN")]
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
    /// Discover dotfiles repo on GitHub
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
        #[arg(short='t', long = "tag", env="HERMIT_TAGS", value_name = "TAG", num_args = 0..)]
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
        /// Override the fallback behavior for existing files
        #[arg(short = 'f', long, value_enum)]
        fallback: Option<FallbackOperation>,
        /// Same as -f backupoverwrite
        #[arg(short = 'F', long)]
        force: bool,
        /// Run actions in parallel
        #[arg(long, default_value_t = false)]
        parallel: bool,
    },
    /// Show status of managed files
    Status {
        /// Include actions matching these tags (can be specified multiple times)
        #[arg(short='t', long = "tag", env="HERMIT_TAGS", value_name = "TAG", num_args = 0..)]
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
        /// Show status of all files, not just those with issues
        #[arg(short = 'e', long, global = true, default_value_t = false)]
        extensive: bool,
    },
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
    /// Add actions to an existing configuration
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
    json: Option<PathBuf>,
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
                }
            }
            InitCommand::Create => {
                cmd_init::create_local_repo(&global_config)?;
            }
        },
        Commands::Add { add_command } => match add_command {
            AddCommand::Config {
                ref config_dir,
                ref required_tags,
            } => {
                cmd_add::add_config(config_dir, required_tags, &[], &[], &[], &global_config)?;
            }
            AddCommand::Link {
                ref config_dir,
                ref source,
                ref link_type,
                ref target,
                ref required_tags,
                ref fallback,
            } => {
                cmd_add::add_link(
                    config_dir,
                    source,
                    link_type,
                    target,
                    required_tags,
                    fallback,
                    &global_config,
                )?;
            }
            AddCommand::Patch {
                ref config_dir,
                ref source,
                ref patch_type,
                ref target,
                ref required_tags,
            } => {
                cmd_add::add_patch(
                    config_dir,
                    source,
                    patch_type,
                    target,
                    required_tags,
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
            ref fallback,
            force,
            parallel,
        } => {
            let fallback = if force {
                Some(FallbackOperation::BackupOverwrite)
            } else {
                *fallback
            };
            let cli = CliOptions {
                fallback,
                confirm,
                verbose,
                tags: tags.clone(),
                profile: profile.clone(),
                json: json.clone(),
            };
            if interactive {
                todo!("Interactive apply is not yet implemented");
            } else {
                cmd_apply::apply_with_tags(&global_config, &cli, parallel).await?;
            }
        }
        Commands::Status {
            extensive,
            ref tags,
            ref profile,
        } => {
            let cli = CliOptions {
                tags: tags.clone(),
                profile: profile.clone(),
                json: json.clone(),
                ..Default::default()
            };
            cmd_status::get_status(&global_config, !extensive, &cli)?;
        }
        Commands::Get { get_command } => match get_command {
            GetCommand::Tags => {
                hermitgrab_info("All tags as required in the configuration:");
                for t in global_config.all_required_tags() {
                    info!("- {t}");
                }
                hermitgrab_info("All built-in detected:");
                for t in detector::detect_builtin_tags() {
                    info!("- {t}");
                }
                hermitgrab_info("All tags by detectors in the configuration:");
                for t in detector::get_detected_tags(&global_config)? {
                    info!("- {t}");
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
            GetCommand::Config => {
                let mut config_map = BTreeMap::new();
                for (config_name, config) in global_config.subconfigs().into_iter() {
                    config_map.insert(config_name, config.clone());
                }

                let formatted = serde_yml::to_string(&config_map)?;
                info("Printing the complete configuration:");
                println!("{}", formatted);
                if let Some(json_path) = &json {
                    std::fs::write(json_path, serde_json::to_string_pretty(&config_map)?)?;
                    info!("Configuration written to {}", json_path.display());
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
