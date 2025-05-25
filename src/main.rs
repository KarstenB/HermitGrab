use anyhow::Result;
use clap::{Parser, Subcommand};
use cmd_apply::apply;
use serde::Deserialize;
use serde::Deserializer;
use std::collections::HashMap;
use std::fs;

pub mod action;
pub mod atomic_link;
pub mod hermitgrab_error;

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
    match cli.command {
        Commands::Init { repo } => {
            crate::cmd_init::run(repo)?;
        }
        Commands::Apply => {
            apply(cli)?;
        }
        Commands::Status => {
            println!("[hermitgrab] Status:");
            // TODO: Implement status reporting
        }
    }
    Ok(())
}


