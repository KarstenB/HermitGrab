use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::LinkType;
use crate::RequireTag;
use crate::atomic_link;
use crate::hermitgrab_error::ActionError;
use crate::hermitgrab_error::InstallActionError;
use crate::hermitgrab_error::LinkActionError;
use handlebars::Handlebars;

#[derive(Debug, Clone)]
pub struct ActionOutput {
    standard_output: String,
    error_output: String,
}

impl ActionOutput {
    pub fn new(standard_output: String, error_output: String) -> Self {
        Self {
            standard_output,
            error_output,
        }
    }

    pub fn standard_output(&self) -> &str {
        &self.standard_output
    }

    pub fn error_output(&self) -> &str {
        &self.error_output
    }
}
pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn get_output(&self) -> Option<ActionOutput> {
        None
    }
    fn tags(&self) -> &[RequireTag];
    fn dependencies(&self) -> &[String];
    fn id(&self) -> String; // Unique identifier for sorting/deps
    fn execute(&self) -> Result<(), ActionError>;
}

pub struct AtomicLinkAction {
    id: String,
    rel_src: String,
    rel_dst: String,
    src: PathBuf,
    dst: String,
    link_type: LinkType,
    tags: Vec<RequireTag>,
    depends: Vec<String>,
}
impl AtomicLinkAction {
    pub(crate) fn new(
        id: String,
        config_dir: &PathBuf,
        src: PathBuf,
        dst: String,
        tags: Vec<RequireTag>,
        depends: Vec<String>,
        link_type: LinkType,
    ) -> Self {
        let rel_src= src
            .strip_prefix(config_dir)
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        let rel_dst = shellexpand::tilde(&dst).to_string();
        let rel_dst = rel_dst
            .strip_prefix(shellexpand::tilde("~/").as_ref())
            .unwrap_or(&rel_dst)
            .to_string();
            
        Self {
            id,
            src,
            rel_src,
            dst,
            rel_dst,
            link_type,
            tags,
            depends,
        }
    }
}

impl Action for AtomicLinkAction {
    fn short_description(&self) -> String {
        format!("Link 🐚/{} -> 🏠/{}", self.rel_src, self.rel_dst)
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
    output: Mutex<Option<ActionOutput>>,
}
impl InstallAction {
    #[allow(clippy::too_many_arguments)]
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
            output: Mutex::new(None),
        }
    }

    fn install_required(&self) -> Result<bool, ActionError> {
        if let Some(check_cmd) = &self.check_cmd {
            let status = execute_script(check_cmd);
            // We ignore errors here which may be caused by the command not being found
            // or other issues, as we only care about successful execution.
            if let Ok(output) = status {
                if output.status.success() {
                    let action_output = ActionOutput::new(
                        "Skipped installation because check passed".to_string(),
                        "".to_string(),
                    );
                    *self.output.lock().expect("Expected to unlock output mutex") =
                        Some(action_output);
                    return Ok(false); // Already installed
                }
            }
        }
        Ok(true)
    }

    fn prepare_install_cmd(&self) -> Result<String, ActionError> {
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
        Ok(cmd)
    }
}

impl Action for InstallAction {
    fn short_description(&self) -> String {
        format!("Install {}", self.name)
    }
    fn long_description(&self) -> String {
        format!("Install {} with cmd: {:?})", self.name, self.install_cmd)
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
        if !self.install_required()? {
            return Ok(()); // Installation not required
        }
        let cmd = self.prepare_install_cmd()?;
        let output = execute_script(&cmd);
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let action_output = ActionOutput::new(stdout, stderr);
                *self.output.lock().expect("Expected to unlock output mutex") = Some(action_output);
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
    fn get_output(&self) -> Option<ActionOutput> {
        self.output
            .lock()
            .expect("Expected to unlock output mutex")
            .clone()
    }
}

fn execute_script(cmd: &str) -> Result<std::process::Output, std::io::Error> {
    if !cmd.starts_with("#!") {
        return std::process::Command::new("sh").arg("-c").arg(cmd).output();
    };
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
