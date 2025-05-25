use std::collections::HashMap;
use std::path::PathBuf;

use crate::RequireTag;
use crate::atomic_link;
use crate::hermitgrab_error::AtomicLinkError;
use anyhow::Result;
use handlebars::Handlebars;

#[derive(Debug, thiserror::Error)]
pub enum HermitGrabError {
    #[error("Atomic link error: {0}")]
    AtomicLink(#[from] AtomicLinkError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Install error: {0}")]
    Install(String),
    #[error(transparent)]
    RenderError(#[from] handlebars::RenderError),
}

pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn tags(&self) -> &[RequireTag];
    fn dependencies(&self) -> &[String];
    fn id(&self) -> String; // Unique identifier for sorting/deps
    fn execute(&self) -> Result<(), HermitGrabError>;
}

pub struct AtomicLinkAction {
    pub id: String,
    pub src: PathBuf,
    pub dst: String,
    pub tags: Vec<RequireTag>,
    pub depends: Vec<String>,
}

impl Action for AtomicLinkAction {
    fn short_description(&self) -> String {
        format!("Link {} -> {}", self.src.display(), self.dst)
    }
    fn long_description(&self) -> String {
        format!(
            "Symlink/copy/link from {} to {} (tags: {:?})",
            self.src.display(), self.dst, self.tags
        )
    }
    fn tags(&self) -> &[RequireTag] {
        &self.tags
    }
    fn dependencies(&self) -> &[String] {
        &self.depends
    }
    fn id(&self) -> String {
        self.id.clone()
    }
    fn execute(&self) -> Result<(), HermitGrabError> {
        let dst_str = shellexpand::tilde(&self.dst).to_string();
        let dst = std::path::Path::new(&dst_str);
        let parent = dst.parent();
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent)?;
        }
        atomic_link::atomic_symlink(&self.src, dst)?;
        Ok(())
    }
}

pub struct InstallAction {
    pub id: String,
    pub name: String,
    pub tags: Vec<RequireTag>,
    pub depends: Vec<String>,
    pub check_cmd: Option<String>,
    pub install_cmd: String,
    pub version: Option<String>,
    pub variables: HashMap<String, String>,
}

impl Action for InstallAction {
    fn short_description(&self) -> String {
        format!("Install {}", self.name)
    }
    fn long_description(&self) -> String {
        format!("Install {} (tags: {:?})", self.name, self.tags)
    }
    fn tags(&self) -> &[RequireTag] {
        &self.tags
    }
    fn dependencies(&self) -> &[String] {
        &self.depends
    }
    fn id(&self) -> String {
        self.id.clone()
    }
    fn execute(&self) -> Result<(), HermitGrabError> {
        // Check if already installed
        if let Some(check_cmd) = &self.check_cmd {
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(check_cmd)
                .status();
            if let Ok(status) = status {
                if status.success() {
                    return Ok(()); // Already installed
                }
            }
        }
        // Install using the specified source
        let reg = Handlebars::new();
        let mut data = self.variables.clone();
        data.insert("name".to_string(), self.name.clone());
        if let Some(version) = &self.version {
            data.insert("version".to_string(), version.clone());
        }
        let template = shellexpand::tilde(&self.install_cmd).to_string();
        let cmd = reg.render_template(&template, &data)?;
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .status();
        if let Ok(status) = status {
            if status.success() {
                Ok(())
            } else {
                Err(HermitGrabError::Install(format!(
                    "Install command failed for {} (exit code: {:?})",
                    self.name,
                    status.code()
                )))
            }
        } else {
            Err(HermitGrabError::Install(format!(
                "Failed to run install command for {}",
                self.name
            )))
        }
    }
}
