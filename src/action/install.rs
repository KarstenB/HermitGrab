// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::io::Write;
use std::process::Output;
use std::sync::{Arc, Mutex};

use derive_where::derive_where;
use serde::Serialize;

use crate::action::{Action, ActionObserver, ActionOutput, Status, id_from_hash};
use crate::config::ConfigItem;
use crate::hermitgrab_error::{ActionError, ConfigError, InstallActionError};
use crate::{HermitConfig, InstallConfig, RequireTag};

#[derive(Serialize)]
#[derive_where(Debug, Hash, PartialEq)]
pub struct InstallAction {
    name: String,
    requires: Vec<RequireTag>,
    check_cmd: Option<String>,
    install_cmd: String,
    order: u64,
    #[derive_where(skip)]
    output: Mutex<Option<ActionOutput>>,
}
impl InstallAction {
    pub fn new(install_entry: &InstallConfig, cfg: &HermitConfig) -> Result<Self, ConfigError> {
        let mut variables = install_entry.variables.clone();
        variables.insert("name".to_string(), install_entry.name.clone());
        let check_cmd = install_entry
            .check
            .as_deref()
            .map(|cmd| cfg.render_handlebars(cmd, &variables))
            .transpose()?;
        let install_cmd = cfg.render_handlebars(&install_entry.install, &variables)?;
        let requires = install_entry.get_all_requires(cfg);
        Ok(Self {
            name: install_entry.name.clone(),
            requires: requires.into_iter().collect(),
            check_cmd,
            install_cmd,
            order: install_entry.total_order(cfg),
            output: Mutex::new(None),
        })
    }

    fn install_required(&self) -> Result<bool, ActionError> {
        if let Some(check_cmd) = &self.check_cmd {
            let status = execute_script(check_cmd);
            // We ignore errors here which may be caused by the command not being found
            // or other issues, as we only care about successful execution.
            if let Ok(output) = status
                && output.status.success()
            {
                self.update_output(check_cmd, output, "check_cmd")?;
                return Ok(false);
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
    fn execute(&self, observer: &Arc<impl ActionObserver>) -> Result<(), ActionError> {
        observer.action_progress(&self.id(), 0, 2, "Checking installation");
        if !self.install_required()? {
            observer.action_progress(&self.id(), 2, 2, "Installation not required");
            return Ok(()); // Installation not required
        }
        observer.action_progress(&self.id(), 1, 2, "Executing installation command");
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
        observer.action_progress(&self.id(), 2, 2, "Installation completed");
        Ok(())
    }
    fn get_output(&self) -> Option<ActionOutput> {
        self.output
            .lock()
            .expect("Expected to unlock output mutex")
            .clone()
    }
    fn id(&self) -> String {
        id_from_hash(self)
    }

    fn get_status(&self, _cfg: &HermitConfig, _quick: bool) -> Status {
        match self.install_required() {
            Ok(false) => Status::Ok(format!("{} is installed", self.name)),
            Ok(true) => Status::NotOk(format!("{} is not installed", self.name)),
            Err(e) => Status::Error(format!("Failed to check installation: {e}")),
        }
    }

    fn get_order(&self) -> u64 {
        self.order
    }
}

pub fn execute_script(cmd: &str) -> Result<Output, std::io::Error> {
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
            writeln!(file, "{cmd}")?;
            file.flush()?;
            let cmd_path = file.into_temp_path();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cmd_path, std::fs::Permissions::from_mode(0o755))?;
            }
            Ok(cmd_path)
        })
        .and_then(|cmd| {
            std::process::Command::new("sh")
                .arg(cmd)
                .env("PATH", path)
                .output()
        })
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
    use std::path::PathBuf;

    use super::*;
    use crate::config::GlobalConfig;

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
            "Expected no stderr output, got: {stderr}"
        );
    }

    #[test]
    fn test_stable_hash_generation() {
        let global_cfg = Arc::new(GlobalConfig::default());
        let path_buf = PathBuf::from("hermit.toml");
        let config = HermitConfig::create_new(path_buf.as_path(), Arc::downgrade(&global_cfg));
        let install_config = InstallConfig {
            name: "Hello World".to_string(),
            check: Some("true".to_string()),
            ..Default::default()
        };
        let action = InstallAction::new(&install_config, &config).unwrap();
        let id = id_from_hash(&action);
        assert_eq!(
            "hermitgrab::action::install::InstallAction:7370f721c8e5df3a",
            &id
        );
    }
}
