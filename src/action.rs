use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::LinkType;
use crate::RequireTag;
use crate::config::PatchType;
use crate::config::Tag;
use crate::hermitgrab_error::ActionError;
use crate::hermitgrab_error::InstallActionError;
use crate::hermitgrab_error::LinkActionError;
use crate::hermitgrab_error::PatchActionError;
use crate::links_files;
use crate::links_files::FallbackOperation;
use handlebars::Handlebars;
use jsonc_parser::ParseOptions;

pub fn expand_directory(dir: &str) -> String {
    let handlebars = handlebars::Handlebars::new();
    let dir = handlebars
        .render_template(dir, &HashMap::<String, String>::new())
        .unwrap_or_else(|_| dir.to_string());

    if dir.starts_with("~/.config") && std::env::var("XDG_CONFIG_HOME").is_ok() {
        dir.replace(
            "~/.config",
            &std::env::var("XDG_CONFIG_HOME").unwrap_or_default(),
        )
    } else if dir.starts_with("~/.local/share") && std::env::var("XDG_DATA_HOME").is_ok() {
        dir.replace(
            "~/.local/share",
            &std::env::var("XDG_DATA_HOME").unwrap_or_default(),
        )
    } else if dir.starts_with("~/.local/state") && std::env::var("XDG_STATE_HOME").is_ok() {
        dir.replace(
            "~/.local/state",
            &std::env::var("XDG_STATE_HOME").unwrap_or_default(),
        )
    } else {
        shellexpand::tilde(&dir).to_string()
    }
}

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
    fn requires(&self) -> &[RequireTag];
    fn provides(&self) -> &[Tag];
    fn provides_tag(&self, tag: &Tag) -> bool {
        self.provides().iter().any(|t| t == tag)
    }
    fn id(&self) -> String; // Unique identifier for sorting/deps
    fn execute(&self) -> Result<(), ActionError>;
}

pub struct LinkAction {
    id: String,
    rel_src: String,
    rel_dst: String,
    src: PathBuf,
    dst: String,
    link_type: LinkType,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
    fallback: FallbackOperation,
}
impl LinkAction {
    pub(crate) fn new(
        id: String,
        config_dir: &PathBuf,
        src: PathBuf,
        dst: String,
        requires: BTreeSet<RequireTag>,
        provides: BTreeSet<Tag>,
        link_type: LinkType,
        fallback: FallbackOperation,
    ) -> Self {
        let rel_src = src
            .strip_prefix(config_dir)
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        let rel_dst = expand_directory(&dst);
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
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
            fallback,
        }
    }
}

impl Action for LinkAction {
    fn short_description(&self) -> String {
        let link_type_str = match self.link_type {
            LinkType::Soft => "Symlink",
            LinkType::Hard => "Hardlink",
            LinkType::Copy => "Copy",
        };
        format!("{link_type_str} {} -> {}", self.rel_src, self.rel_dst)
    }
    fn long_description(&self) -> String {
        format!(
            "Symlink from {} to {} (tags: {:?})",
            self.src.display(),
            self.dst,
            self.requires
        )
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
        let dst_str = expand_directory(&self.dst);
        links_files::link_files(&self.src, dst_str, &self.link_type, &self.fallback)
            .map_err(LinkActionError::AtomicLinkError)?;
        Ok(())
    }
}

pub struct PatchAction {
    id: String,
    rel_src: String,
    rel_dst: String,
    src: PathBuf,
    dst: String,
    patch_type: PatchType,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
}

impl PatchAction {
    pub(crate) fn new(
        id: String,
        config_dir: &PathBuf,
        src: PathBuf,
        dst: String,
        requires: BTreeSet<RequireTag>,
        provides: BTreeSet<Tag>,
        patch_type: PatchType,
    ) -> Self {
        let rel_src = src
            .strip_prefix(config_dir)
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        let rel_dst = expand_directory(&dst);
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
            patch_type,
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
        }
    }
}

impl Action for PatchAction {
    fn short_description(&self) -> String {
        format!("{} {} with {}", self.patch_type, self.rel_dst, self.rel_src)
    }

    fn long_description(&self) -> String {
        format!("{} {} with {:?}", self.patch_type, self.dst, self.src)
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
        match self.patch_type {
            PatchType::JsonMerge => {
                let dst = expand_directory(&self.dst);
                merge_json(&self.src, PathBuf::from(dst))?;
                Ok(())
            }
            PatchType::JsonPatch => {
                let dst = expand_directory(&self.dst);
                patch_json(&self.src, &PathBuf::from(dst))?;
                Ok(())
            }
        }
    }
}

pub fn merge_json(src: &PathBuf, dst: PathBuf) -> Result<ActionOutput, PatchActionError> {
    let (merge_content, _) = content_and_extension(&src)?;
    let (mut dst_content, lower_case_ext) = content_and_extension(&dst)?;
    json_patch::merge(&mut dst_content, &merge_content);
    let updated_dst = to_content(dst_content, &lower_case_ext)?;
    write_contents(&dst, updated_dst)?;
    Ok(ActionOutput::new(
        format!("Merged the contents of {src:?} into {dst:?}"),
        String::new(),
    ))
}

fn write_contents(dst: &PathBuf, updated_dst: String) -> Result<(), PatchActionError> {
    let dst_dir = dst.parent().expect("Failed to get parent directory");
    if !dst_dir.exists() {
        std::fs::create_dir_all(dst_dir)?;
    }
    std::fs::write(dst, updated_dst)?;
    Ok(())
}

pub fn patch_json(src: &PathBuf, dst: &PathBuf) -> Result<ActionOutput, PatchActionError> {
    let (merge_content, _) = content_and_extension(&src)?;
    let patch: json_patch::Patch = serde_json::from_value(merge_content)?;
    let (mut dst_json, lower_case_ext) = content_and_extension(dst)?;
    json_patch::patch(&mut dst_json, &patch)?;
    let updated_dst = to_content(dst_json, &lower_case_ext)?;
    write_contents(dst, updated_dst)?;
    Ok(ActionOutput::new(
        format!("Merged the contents of {src:?} into {dst:?}"),
        String::new(),
    ))
}

fn content_and_extension(
    dst: &PathBuf,
) -> Result<(serde_json::Value, Option<String>), PatchActionError> {
    let dst_content = if dst.exists() {
        std::fs::read_to_string(dst).map_err(PatchActionError::IoError)?
    } else {
        "".to_string()
    };
    let lower_case_ext = dst
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase());
    Ok((parse_file(dst_content, &lower_case_ext)?, lower_case_ext))
}

fn to_content(
    dst_json: serde_json::Value,
    extension: &Option<String>,
) -> Result<String, PatchActionError> {
    match extension.as_deref() {
        Some("yaml") | Some("yml") => {
            let yaml = serde_yml::to_string(&dst_json)?;
            return Ok(yaml);
        }
        Some("toml") => {
            let toml = toml::to_string_pretty(&dst_json)?;
            return Ok(toml);
        }
        _ => {
            let json = serde_json::to_string_pretty(&dst_json)?;
            return Ok(json);
        }
    }
}

fn parse_file(
    dst_content: String,
    extension: &Option<String>,
) -> Result<serde_json::Value, PatchActionError> {
    match extension.as_deref() {
        Some("yaml") | Some("yml") => {
            let yaml: serde_yml::Value = serde_yml::from_str(&dst_content)?;
            return Ok(serde_json::to_value(yaml)?);
        }
        Some("toml") => {
            let toml: toml::Value = toml::from_str(&dst_content)?;
            return Ok(serde_json::to_value(toml)?);
        }
        _ => {
            let value = jsonc_parser::parse_to_serde_value(&dst_content, &ParseOptions::default())?;
            if let Some(value) = value {
                return Ok(value);
            }
            return Ok(serde_json::from_str(&dst_content)?);
        }
    }
}

pub struct InstallAction {
    id: String,
    name: String,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
    check_cmd: Option<String>,
    pre_install_cmd: Option<String>,
    post_install_cmd: Option<String>,
    install_cmd: String,
    version: Option<String>,
    variables: BTreeMap<String, String>,
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
        variables: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id,
            name,
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
            check_cmd,
            pre_install_cmd,
            post_install_cmd,
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
                        format!("Skipped installation of {} because check passed", self.name),
                        "".to_string(),
                    );
                    *self.output.lock().expect("Expected to unlock output mutex") =
                        Some(action_output);
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn prepare_cmd(&self, cmd: &str) -> Result<String, ActionError> {
        let reg = Handlebars::new();
        let mut data = self.variables.clone();
        data.insert("name".to_string(), self.name.clone());
        if let Some(version) = &self.version {
            data.insert("version".to_string(), version.clone());
        }
        let template = shellexpand::tilde(cmd).to_string();
        let cmd = reg
            .render_template(&template, &data)
            .map_err(InstallActionError::RenderError)?;
        Ok(cmd)
    }

    fn update_output(&self, cmd: &String, output: std::process::Output) -> Result<(), ActionError> {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut mutex_guard = self.output.lock().expect("Expected to unlock output mutex");
        if let Some(action_output) = mutex_guard.as_ref() {
            // If output is already set, append to it
            let new_output = ActionOutput::new(
                format!("{}---\n{}", action_output.standard_output, stdout),
                format!("{}---\n{}", action_output.error_output, stderr),
            );
            *mutex_guard = Some(new_output);
        } else {
            // Otherwise, create a new output
            let action_output = ActionOutput::new(stdout, stderr);
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
            let cmd = self.prepare_cmd(pre_cmd)?;
            let output = execute_script(&cmd);
            match output {
                Ok(output) => {
                    self.update_output(&cmd, output)?;
                }
                Err(e) => Err(InstallActionError::PreCommandFailedLaunch(cmd, e))?,
            }
        }
        let cmd = self.prepare_cmd(&self.install_cmd)?;
        let output = execute_script(&cmd);
        match output {
            Ok(output) => {
                self.update_output(&cmd, output)?;
            }
            Err(e) => Err(InstallActionError::CommandFailedLaunch(cmd, e))?,
        }
        if let Some(ref post_cmd) = self.post_install_cmd {
            let cmd = self.prepare_cmd(post_cmd)?;
            let output = execute_script(&cmd);
            match output {
                Ok(output) => {
                    self.update_output(&cmd, output)?;
                }
                Err(e) => Err(InstallActionError::PostCommandFailedLaunch(cmd, e))?,
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

fn execute_script(cmd: &str) -> Result<std::process::Output, std::io::Error> {
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
    fn test_handlebars_with_dot() {
        let handlebars = handlebars::Handlebars::new();
        let mut hash_map = HashMap::<String, String>::new();
        hash_map.insert("hermit_thisdir".to_string(), "test".to_owned());
        let dir = "{{hermit_thisdir}}/bla";
        let dir = handlebars
            .render_template(dir, &hash_map)
            .unwrap_or_else(|_| dir.to_string());
        assert_eq!(dir, "test/bla");
    }

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
