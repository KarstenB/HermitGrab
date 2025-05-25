use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::LinkType;
use crate::RequireTag;
use crate::atomic_link;
use crate::hermitgrab_error::ActionError;
use crate::hermitgrab_error::InstallActionError;
use crate::hermitgrab_error::LinkActionError;
use handlebars::Handlebars;

pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn tags(&self) -> &[RequireTag];
    fn dependencies(&self) -> &[String];
    fn id(&self) -> String; // Unique identifier for sorting/deps
    fn execute(&self) -> Result<(), ActionError>;
}

pub struct AtomicLinkAction {
    id: String,
    src: PathBuf,
    dst: String,
    link_type: LinkType,
    tags: Vec<RequireTag>,
    depends: Vec<String>,
}
impl AtomicLinkAction {
    pub(crate) fn new(
        id: String,
        src: PathBuf,
        dst: String,
        tags: Vec<RequireTag>,
        depends: Vec<String>,
        link_type: LinkType,
    ) -> Self {
        Self {
            id,
            src,
            dst,
            link_type,
            tags,
            depends,
        }
    }
}

impl Action for AtomicLinkAction {
    fn short_description(&self) -> String {
        format!("Link {} -> {}", self.src.display(), self.dst)
    }
    fn long_description(&self) -> String {
        format!(
            "Symlink from {} to {} (tags: {:?})",
            self.src.display(),
            self.dst,
            self.tags
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
    fn execute(&self) -> Result<(), ActionError> {
        let dst_str = shellexpand::tilde(&self.dst).to_string();
        atomic_link::atomic_symlink(&self.src, dst_str, self.link_type)
            .map_err(LinkActionError::AtomicLinkError)?;
        Ok(())
    }
}

pub struct InstallAction {
    id: String,
    name: String,
    tags: Vec<RequireTag>,
    depends: Vec<String>,
    check_cmd: Option<String>,
    install_cmd: String,
    version: Option<String>,
    variables: HashMap<String, String>,
}
impl InstallAction {
    pub fn new(
        id: String,
        name: String,
        tags: Vec<RequireTag>,
        depends: Vec<String>,
        check_cmd: Option<String>,
        install_cmd: String,
        version: Option<String>,
        variables: HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            name,
            tags,
            depends,
            check_cmd,
            install_cmd,
            version,
            variables,
        }
    }
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
    fn execute(&self) -> Result<(), ActionError> {
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
        let cmd = reg
            .render_template(&template, &data)
            .map_err(InstallActionError::RenderError)?;
        let output = if cmd.starts_with("#!") {
            execute_script(&cmd)
        } else {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
        };
        match output {
            Ok(output) => {
                let status = output.status;
                if status.success() {
                    Ok(())
                } else {
                    Err(InstallActionError::CommandFailed(
                        cmd,
                        status.code().expect("Should have an exit code"),
                    ))?
                }
            }
            Err(e) => Err(InstallActionError::CommandFailedLaunch(cmd, e))?,
        }
    }
}

fn execute_script(cmd: &str) -> Result<std::process::Output, std::io::Error> {
    tempfile::NamedTempFile::new()
        .and_then(|mut file| {
            writeln!(file, "{}", cmd)?;
            file.flush()?;
            let cmd_path = file.into_temp_path();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cmd_path, std::fs::Permissions::from_mode(0o755))?;
            }
            Ok(cmd_path)
        })
        .and_then(|cmd| std::process::Command::new(&cmd).output())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_execution() {
        let script = r#"
            echo "Hello, World!"
            exit 0
        "#;
        let status = execute_script(script);
        assert!(status.is_ok());
        let output = status.unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.trim(), "Hello, World!");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.is_empty(),
            "Expected no stderr output, got: {}",
            stderr
        );
    }
}
