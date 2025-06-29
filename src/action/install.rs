use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::Output,
    sync::Mutex,
};

use crate::{
    HermitConfig, RequireTag,
    action::{Action, ActionOutput},
    config::Tag,
    hermitgrab_error::{ActionError, ConfigLoadError, InstallActionError},
};

pub struct InstallAction {
    id: String,
    name: String,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
    check_cmd: Option<String>,
    pre_install_cmd: Option<String>,
    post_install_cmd: Option<String>,
    install_cmd: String,
    output: Mutex<Option<ActionOutput>>,
}
impl InstallAction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        requires: BTreeSet<RequireTag>,
        provides: BTreeSet<Tag>,
        check_cmd: Option<String>,
        pre_install_cmd: Option<String>,
        post_install_cmd: Option<String>,
        install_cmd: String,
        version: Option<String>,
        mut variables: BTreeMap<String, String>,
        cfg: &HermitConfig,
    ) -> Result<Self, ConfigLoadError> {
        variables.insert("name".to_string(), name.clone());
        variables.insert("version".to_string(), version.clone().unwrap_or_default());
        let pre_install_cmd = pre_install_cmd
            .map(|cmd| cfg.global_config().prepare_cmd(&cmd, &variables))
            .transpose()?;
        let post_install_cmd = post_install_cmd
            .map(|cmd| cfg.global_config().prepare_cmd(&cmd, &variables))
            .transpose()?;
        let check_cmd = check_cmd
            .map(|cmd| cfg.global_config().prepare_cmd(&cmd, &variables))
            .transpose()?;
        let install_cmd = cfg.global_config().prepare_cmd(&install_cmd, &variables)?;
        Ok(Self {
            id,
            name,
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
            check_cmd,
            pre_install_cmd,
            post_install_cmd,
            install_cmd,
            output: Mutex::new(None),
        })
    }

    fn install_required(&self) -> Result<bool, ActionError> {
        if let Some(check_cmd) = &self.check_cmd {
            let status = execute_script(check_cmd);
            // We ignore errors here which may be caused by the command not being found
            // or other issues, as we only care about successful execution.
            if let Ok(output) = status {
                if output.status.success() {
                    self.update_output(check_cmd, output, "check_cmd")?;
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn update_output(&self, cmd: &String, output: Output, name: &str) -> Result<(), ActionError> {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = stdout.trim();
        let stderr = stderr.trim();
        if stderr.is_empty() && stdout.is_empty() {
            return Ok(());
        }
        let mut mutex_guard = self.output.lock().expect("Expected to unlock output mutex");
        if let Some(action_output) = mutex_guard.as_mut() {
            // If output is already set, append to it
            action_output.add(name, stdout, stderr);
        } else {
            // Otherwise, create a new output
            let mut action_output = ActionOutput::default();
            action_output.add(name, stdout, stderr);
            *mutex_guard = Some(action_output);
        }
        let status = output.status;
        if status.success() {
            Ok(())
        } else {
            Err(InstallActionError::CommandFailed(
                cmd.to_string(),
                status.code().expect("Should have an exit code"),
            ))?
        }
    }
}

impl Action for InstallAction {
    fn short_description(&self) -> String {
        format!("Install {}", self.name)
    }
    fn long_description(&self) -> String {
        format!("Install {} with cmd: {:?})", self.name, self.install_cmd)
    }
    fn requires(&self) -> &[RequireTag] {
        &self.requires
    }
    fn provides(&self) -> &[Tag] {
        &self.provides
    }
    fn id(&self) -> String {
        self.id.clone()
    }
    fn execute(&self) -> Result<(), ActionError> {
        if !self.install_required()? {
            return Ok(()); // Installation not required
        }
        if let Some(ref pre_cmd) = self.pre_install_cmd {
            let output = execute_script(pre_cmd);
            match output {
                Ok(output) => {
                    self.update_output(pre_cmd, output, "pre_cmd")?;
                }
                Err(e) => Err(InstallActionError::PreCommandFailedLaunch(
                    pre_cmd.to_string(),
                    e,
                ))?,
            }
        }
        let output = execute_script(&self.install_cmd);
        match output {
            Ok(output) => {
                self.update_output(&self.install_cmd, output, "install_cmd")?;
            }
            Err(e) => Err(InstallActionError::CommandFailedLaunch(
                self.install_cmd.clone(),
                e,
            ))?,
        }
        if let Some(ref post_cmd) = self.post_install_cmd {
            let output = execute_script(post_cmd);
            match output {
                Ok(output) => {
                    self.update_output(post_cmd, output, "post_cmd")?;
                }
                Err(e) => Err(InstallActionError::PostCommandFailedLaunch(
                    post_cmd.to_string(),
                    e,
                ))?,
            }
        }
        Ok(())
    }
    fn get_output(&self) -> Option<ActionOutput> {
        self.output
            .lock()
            .expect("Expected to unlock output mutex")
            .clone()
    }
}

fn execute_script(cmd: &str) -> Result<Output, std::io::Error> {
    let path = if which::which("ubi").is_err() {
        insert_ubi_into_path()?
    } else {
        std::env::var("PATH").unwrap_or_default()
    };
    if !cmd.starts_with("#!") {
        return std::process::Command::new("sh")
            .env("PATH", path)
            .arg("-c")
            .arg(cmd)
            .output();
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
        .and_then(|cmd| std::process::Command::new(&cmd).env("PATH", path).output())
}

#[cfg(not(feature = "ubi"))]
fn insert_ubi_into_path() -> Result<String, std::io::Error> {
    Ok(std::env::var("PATH").unwrap_or_default())
}

#[cfg(feature = "ubi")]
fn insert_ubi_into_path() -> Result<String, std::io::Error> {
    use std::sync::OnceLock;
    use tempfile::TempDir;
    static UBI_EXE_DIR: OnceLock<TempDir> = OnceLock::new();
    let temp_dir =
        UBI_EXE_DIR.get_or_init(|| TempDir::new().expect("Failed to create temporary directory"));
    let mut path = std::env::var("PATH").unwrap_or_default();
    path.push(':');
    path.push_str(temp_dir.path().to_str().unwrap());
    let ubi_exe = temp_dir.path().join("ubi");
    if !ubi_exe.exists() {
        let script = format!(
            r#"#!/bin/sh
{} ubi -- "$@"
        "#,
            std::env::current_exe()?.display()
        );
        std::fs::write(&ubi_exe, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ubi_exe, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(path)
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
