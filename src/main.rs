use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs;

pub mod action;
pub mod atomic_link;
pub mod hermitgrab_error;

pub use crate::action::{Action, AtomicLinkAction, InstallAction};
pub use crate::cmd_apply::run as apply_command;
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
    pub tags: Option<Vec<String>>,
    pub files: Vec<DotfileEntry>,
    pub install: Option<Vec<InstallEntry>>,
    pub sources: Option<HashMap<String, String>>,
    pub depends: Option<Vec<String>>,
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
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstallEntry {
    pub name: String,
    pub check_cmd: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

impl InstallEntry {
    pub fn get(&self, key: &str) -> Option<&str> {
        match key {
            "name" => Some(self.name.as_str()),
            "check_cmd" => self.check_cmd.as_deref(),
            "source" => self.source.as_deref(),
            "version" => self.version.as_deref(),
            _ => self.variables.get(key).map(|s| s.as_str()),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn check_cmd(&self) -> Option<&str> {
        self.check_cmd.as_deref()
    }
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
    pub fn tags(&self) -> Option<&Vec<String>> {
        self.tags.as_ref()
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn variables_map(&self) -> &std::collections::HashMap<String, String> {
        &self.variables
    }
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
            // 1. Find all hermit.yaml files
            let user_dirs = directories::UserDirs::new().expect("Could not get user directories");
            let search_root = user_dirs.home_dir().join(".hermitgrab");
            let yaml_files = crate::cmd_apply::find_hermit_yaml_files(&search_root);
            // 2. Parse all configs
            let mut configs = Vec::new();
            for path in &yaml_files {
                match load_hermit_config(path.to_str().unwrap()) {
                    Ok(cfg) => configs.push((path.clone(), cfg)),
                    Err(e) => eprintln!("[hermitgrab] Error loading {}: {}", path.display(), e),
                }
            }
            // 3. Build actions
            let mut actions: Vec<Arc<dyn Action>> = Vec::new();
            for (path, cfg) in &configs {
                let config_tags = cfg.tags.clone().unwrap_or_default();
                let depends = cfg.depends.clone().unwrap_or_default();
                for file in &cfg.files {
                    let mut tags = config_tags.clone();
                    if let Some(ftags) = &file.tags {
                        tags.extend(ftags.clone());
                    }
                    let id = format!("link:{}:{}", path.display(), file.target);
                    actions.push(Arc::new(AtomicLinkAction {
                        id,
                        src: path
                            .parent()
                            .unwrap()
                            .join(&file.source)
                            .display()
                            .to_string(),
                        dst: file.target.clone(),
                        tags,
                        depends: depends.clone(),
                    }));
                }
                if let Some(installs) = &cfg.install {
                    for inst in installs {
                        let mut tags = config_tags.clone();
                        if let Some(itags) = inst.tags() {
                            tags.extend(itags.iter().cloned());
                        }
                        let id = format!("install:{}:{}", path.display(), inst.name);
                        actions.push(Arc::new(InstallAction {
                            id,
                            name: inst.name.clone(),
                            tags,
                            depends: depends.clone(),
                            check_cmd: inst.check_cmd.clone(),
                            source: inst.source.clone(),
                            version: inst.version.clone(),
                            sources_map: cfg.sources.clone(),
                            variables: inst.variables.clone(),
                        }));
                    }
                }
            }
            // 4. Topo sort actions by dependencies (simple, by id)
            let mut sorted = Vec::new();
            let mut seen = HashSet::new();
            fn visit(
                a: &Arc<dyn Action>,
                actions: &Vec<Arc<dyn Action>>,
                seen: &mut HashSet<String>,
                sorted: &mut Vec<Arc<dyn Action>>,
            ) {
                if seen.contains(&a.id()) {
                    return;
                }
                for dep in a.dependencies() {
                    if let Some(dep_a) = actions.iter().find(|x| &x.id() == dep) {
                        visit(dep_a, actions, seen, sorted);
                    }
                }
                seen.insert(a.id());
                sorted.push(a.clone());
            }
            for a in &actions {
                visit(a, &actions, &mut seen, &mut sorted);
            }
            // 5. Print plan
            println!("[hermitgrab] Execution plan:");
            for (i, a) in sorted.iter().enumerate() {
                println!(
                    "{}. {} [tags: {:?}]",
                    i + 1,
                    a.short_description(),
                    a.tags()
                );
            }
            // 6. Confirm
            use std::io::{self, Write};
            if !cli.verbose {
                print!("Proceed? [y/N]: ");
                io::stdout().flush().unwrap();
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                if !matches!(input.trim(), "y" | "Y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            // 7. Execute plan (sequential for now)
            let mut results = Vec::new();
            for a in &sorted {
                let res = a.execute();
                results.push((a.short_description(), res));
            }
            // 8. Summary
            println!("[hermitgrab] Summary:");
            for (desc, res) in &results {
                match res {
                    Ok(_) => println!("[ok] {}", desc),
                    Err(e) => println!("[err] {}: {}", desc, e),
                }
            }
        }
        Commands::Status => {
            println!("[hermitgrab] Status:");
            // TODO: Implement status reporting
        }
    }
    Ok(())
}
