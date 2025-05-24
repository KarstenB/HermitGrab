use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs;

pub mod atomic_link;
pub mod hermitgrab_error;

pub use crate::cmd_apply::run as apply_command;
pub use crate::cmd_init::run as init_command;
pub use crate::hermitgrab_error::AtomicLinkError;

#[derive(Parser)]
#[command(name = "hermitgrab")]
#[command(about = "A modern dotfile manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Increase output verbosity
    #[arg(short, long, global = true)]
    verbose: bool,
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

#[derive(Debug, Deserialize)]
pub struct HermitConfig {
    pub files: Vec<DotfileEntry>,
    pub install: Option<Vec<InstallEntry>>,
    pub sources: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstallEntry(pub HashMap<String, String>);

impl InstallEntry {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }
    pub fn name(&self) -> Option<&str> {
        self.get("name")
    }
    pub fn check_cmd(&self) -> Option<&str> {
        self.get("check_cmd")
    }
    pub fn source(&self) -> Option<&str> {
        self.get("source")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    Soft,
    Hard,
    Copy,
}

#[derive(Debug, Deserialize)]
pub struct DotfileEntry {
    pub source: String,
    pub target: String,
    pub link: LinkType,
}

pub fn load_hermit_config(path: &str) -> anyhow::Result<HermitConfig> {
    let content = fs::read_to_string(path)?;
    let config: HermitConfig = serde_yaml::from_str(&content)?;
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
            crate::cmd_apply::run_with_dir(None, cli.verbose)?;
        }
        Commands::Status => {
            println!("[hermitgrab] Status:");
            // TODO: Implement status reporting
        }
    }
    Ok(())
}
