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
}

pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn tags(&self) -> &[String];
    fn dependencies(&self) -> &[String];
    fn id(&self) -> String; // Unique identifier for sorting/deps
    fn execute(&self) -> Result<(), HermitGrabError>;
}

pub struct AtomicLinkAction {
    pub id: String,
    pub src: String,
    pub dst: String,
    pub tags: Vec<String>,
    pub depends: Vec<String>,
}

impl Action for AtomicLinkAction {
    fn short_description(&self) -> String {
        format!("Link {} -> {}", self.src, self.dst)
    }
    fn long_description(&self) -> String {
        format!(
            "Symlink/copy/link from {} to {} (tags: {:?})",
            self.src, self.dst, self.tags
        )
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
    fn dependencies(&self) -> &[String] {
        &self.depends
    }
    fn id(&self) -> String {
        self.id.clone()
    }
    fn execute(&self) -> Result<(), HermitGrabError> {
        let src = std::path::Path::new(&self.src);
        let dst_str = shellexpand::tilde(&self.dst).to_string();
        let dst = std::path::Path::new(&dst_str);
        let parent = dst.parent();
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent)?;
        }
        atomic_link::atomic_symlink(src, dst)?;
        Ok(())
    }
}

pub struct InstallAction {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub depends: Vec<String>,
    pub check_cmd: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    pub sources_map: Option<std::collections::HashMap<String, String>>,
    pub variables: std::collections::HashMap<String, String>, // Add variables field
}

impl Action for InstallAction {
    fn short_description(&self) -> String {
        format!("Install {}", self.name)
    }
    fn long_description(&self) -> String {
        format!("Install {} (tags: {:?})", self.name, self.tags)
    }
    fn tags(&self) -> &[String] {
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
        if let Some(source) = &self.source {
            if let Some(sources_map) = &self.sources_map {
                if let Some(template) = sources_map.get(source) {
                    let reg = Handlebars::new();
                    let mut data = self.variables.clone();
                    data.insert("name".to_string(), self.name.clone());
                    if let Some(version) = &self.version {
                        data.insert("version".to_string(), version.clone());
                    }
                    let cmd = reg.render_template(template, &data).unwrap_or_else(|_| template.clone());
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
                } else {
                    Err(HermitGrabError::Install(format!(
                        "Unknown source: {} for {}",
                        source, self.name
                    )))
                }
            } else {
                Err(HermitGrabError::Install(
                    "No sources map provided for install action".to_string(),
                ))
            }
        } else {
            Err(HermitGrabError::Install(format!(
                "No source specified for install action: {}",
                self.name
            )))
        }
    }
}
