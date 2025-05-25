use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde::Deserializer;
use std::collections::HashMap;
use std::fs;

pub mod action;
pub mod atomic_link;
pub mod hermitgrab_error;
pub mod detector;

pub use crate::action::{Action, AtomicLinkAction, InstallAction};
pub use crate::cmd_init::run as init_command;
pub use crate::hermitgrab_error::AtomicLinkError;
pub use std::collections::HashSet;
pub use std::sync::Arc;

#[derive(Parser)]
#[command(name = "hermitgrab")]
#[command(about = "A modern dotfile manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Increase output verbosity
    #[arg(short, long, global = true)]
    verbose: bool,
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Tag {
    Positive(String),
    Negative(String),
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim();
        if let Some(rest) = trimmed.strip_prefix('+') {
            Ok(Tag::Positive(rest.to_string()))
        } else if let Some(rest) = trimmed.strip_prefix('-') {
            Ok(Tag::Negative(rest.to_string()))
        } else if let Some(rest) = trimmed.strip_prefix('~') {
            Ok(Tag::Negative(rest.to_string()))
        } else {
            Ok(Tag::Positive(trimmed.to_string()))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HermitConfig {
    /// These tags are defined by this config.
    #[serde(default)]
    pub tags: Vec<Tag>,
    /// List of dotfiles to manage
    #[serde(default)]
    pub files: Vec<DotfileEntry>,
    /// List of applications to install
    #[serde(default)]
    pub install: Vec<InstallEntry>,
    /// Sources for install commands, instructions for how to install applications.
    #[serde(default)]
    pub sources: HashMap<String, String>,
    /// Dependencies that also need to be installed.
    #[serde(default)]
    pub depends: Vec<String>,
    /// Profiles: map from profile name to list of tags
    #[serde(default)]
    pub profiles: HashMap<String, Vec<Tag>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    #[default]
    Soft,
    Hard,
    Copy,
}

#[derive(Debug, Deserialize)]
pub struct DotfileEntry {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub link: LinkType,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstallEntry {
    pub name: String,
    pub check_cmd: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

impl InstallEntry {
    pub fn to_handlebars_map(&self) -> std::collections::HashMap<String, String> {
        let mut map = self.variables.clone();
        map.insert("name".to_string(), self.name.clone());
        if let Some(version) = &self.version {
            map.insert("version".to_string(), version.clone());
        }
        map
    }
}

pub fn load_hermit_config(path: &str) -> anyhow::Result<HermitConfig> {
    let content = fs::read_to_string(path)?;
    let config: HermitConfig = serde_yml::from_str(&content)?;
    Ok(config)
}

pub mod cmd_apply;
pub mod cmd_init;

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Detect built-in tags
    let mut detected_tags = detector::detect_builtin_tags();
    // Merge user-supplied tags
    for t in &cli.tags {
        detected_tags.insert(t.clone());
    }
    match cli.command {
        Commands::Init { repo } => {
            crate::cmd_init::run(repo)?;
        }
        Commands::Apply => {
            cmd_apply::apply_with_tags(cli, detected_tags)?;
        }
        Commands::Status => {
            println!("[hermitgrab] Status:");
            // TODO: Implement status reporting
        }
        Commands::Get { get_command } => {
            // 1. Find all hermit.yaml files
            let user_dirs = directories::UserDirs::new().expect("Could not get user directories");
            let search_root = user_dirs.home_dir().join(".hermitgrab");
            let yaml_files = crate::cmd_apply::find_hermit_yaml_files(&search_root);
            let mut configs = Vec::new();
            for path in &yaml_files {
                match load_hermit_config(path.to_str().unwrap()) {
                    Ok(cfg) => configs.push((path.clone(), cfg)),
                    Err(e) => eprintln!("[hermitgrab] Error loading {}: {}", path.display(), e),
                }
            }
            match get_command {
                GetCommand::Tags => {
                    // Collect all tags from configs and detected, lower-case, dedup
                    let mut all_tags = std::collections::HashSet::new();
                    for (_path, cfg) in &configs {
                        for tag in &cfg.tags {
                            match tag {
                                Tag::Positive(t) | Tag::Negative(t) => { all_tags.insert(t.to_lowercase()); }
                            }
                        }
                        for file in &cfg.files {
                            for tag in &file.tags {
                                match tag {
                                    Tag::Positive(t) | Tag::Negative(t) => { all_tags.insert(t.to_lowercase()); }
                                }
                            }
                        }
                        for inst in &cfg.install {
                            for tag in &inst.tags {
                                match tag {
                                    Tag::Positive(t) | Tag::Negative(t) => { all_tags.insert(t.to_lowercase()); }
                                }
                            }
                        }
                        for tags in cfg.profiles.values() {
                            for tag in tags {
                                match tag {
                                    Tag::Positive(t) | Tag::Negative(t) => { all_tags.insert(t.to_lowercase()); }
                                }
                            }
                        }
                    }
                    // Add detected tags
                    for t in &detector::detect_builtin_tags() {
                        all_tags.insert(t.to_lowercase());
                    }
                    let mut all_tags: Vec<_> = all_tags.into_iter().collect();
                    all_tags.sort();
                    println!("All tags (including auto-detected):");
                    for t in all_tags {
                        println!("- {}", t);
                    }
                }
                GetCommand::Profiles => {
                    // Collect all profiles from all configs, lower-case, error on duplicate
                    let mut all_profiles: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
                    for (_path, cfg) in &configs {
                        for (profile, tags) in &cfg.profiles {
                            let profile_lc = profile.to_lowercase();
                            if all_profiles.contains_key(&profile_lc) {
                                eprintln!("[hermitgrab] Error: duplicate profile '{}' found in multiple configs", profile_lc);
                                std::process::exit(1);
                            }
                            let mut entry: Vec<String> = Vec::new();
                            for tag in tags {
                                match tag {
                                    Tag::Positive(t) | Tag::Negative(t) => {
                                        let t_lc = t.to_lowercase();
                                        if !entry.contains(&t_lc) {
                                            entry.push(t_lc);
                                        }
                                    }
                                }
                            }
                            all_profiles.insert(profile_lc, entry);
                        }
                    }
                    println!("All profiles:");
                    for (profile, mut tags) in all_profiles {
                        tags.sort();
                        tags.dedup();
                        println!("- {}: {:?}", profile, tags);
                    }
                }
            }
        }
    }
    Ok(())
}


