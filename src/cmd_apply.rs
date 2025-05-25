use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::{GlobalConfig, Tag};
use crate::hermitgrab_error::ApplyError;
use crate::{Action, Cli};

pub(crate) fn apply_with_tags(
    cli: Cli,
    detected_tags: BTreeSet<Tag>,
    global_config: &GlobalConfig,
) -> Result<(), ApplyError> {
    let active_tags = get_active_tags(&cli, detected_tags, global_config)?;
    println!("[hermitgrab] Active tags: {:?}", active_tags);
    let actions = create_actions(global_config)?;
    let sorted = topological_sort(actions);
    println!("[hermitgrab] Execution plan:");
    for (i, a) in sorted.iter().enumerate() {
        println!(
            "{}. {} [tags: {:?}]",
            i + 1,
            a.short_description(),
            a.tags()
        );
    }

    print!("Proceed? [y/N]: ");
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    if !matches!(input.trim(), "y" | "Y") {
        println!("Aborted.");
        return Ok(());
    }
    let mut results = Vec::new();
    for a in &sorted {
        let res = a.execute();
        results.push((a.short_description(), res));
    }
    println!("[hermitgrab] Summary:");
    for (desc, res) in &results {
        match res {
            Ok(_) => println!("[ok] {}", desc),
            Err(e) => println!("[err] {}: {}", desc, e),
        }
    }
    Ok(())
}

fn get_active_tags(
    cli: &Cli,
    detected_tags: BTreeSet<Tag>,
    global_config: &GlobalConfig,
) -> Result<BTreeSet<Tag>, ApplyError> {
    let profile_to_use = if let Some(profile) = &cli.profile {
        Some(profile.to_lowercase())
    } else if global_config.all_profiles.contains_key("default") {
        Some("default".to_string())
    } else {
        None
    };
    let mut active_tags = detected_tags.clone();
    if let Some(profile) = profile_to_use {
        if let Some(profile_tags) = global_config.all_profiles.get(&profile) {
            active_tags.extend(profile_tags.iter().cloned());
        } else {
            return Err(ApplyError::ProfileNotFound(profile));
        }
    }
    Ok(active_tags)
}

fn topological_sort(actions: Vec<Arc<dyn Action + 'static>>) -> Vec<Arc<dyn Action + 'static>> {
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
    sorted
}

fn create_actions(
    global_config: &GlobalConfig,
) -> Result<Vec<Arc<dyn Action + 'static>>, ApplyError> {
    let mut actions: Vec<Arc<dyn crate::Action>> = Vec::new();
    for cfg in &global_config.subconfigs {
        let depends = &cfg.depends;
        for file in &cfg.files {
            let id = format!("link:{}:{}", cfg.path().display(), file.target);
            let source = cfg.path().parent().expect("File should have a directory").join(&file.source);
            actions.push(Arc::new(crate::AtomicLinkAction {
                id,
                src: source,
                dst: file.target.clone(),
                tags: file.requires.clone(),
                depends: depends.clone(),
            }));
        }
        for inst in &cfg.install {
            // Filter install actions by tags
            let id = format!("install:{}:{}", cfg.path().display(), inst.name);
            // Use global_config.all_sources for install_cmd
            let install_cmd = inst
                .source
                .as_ref()
                .and_then(|src| global_config.all_sources.get(&src.to_lowercase()))
                .or_else(|| global_config.all_sources.get(&inst.name.to_lowercase()));
            let Some(install_cmd) = install_cmd else {
                return Err(ApplyError::InstallSourceNotFound(inst.name.clone()));
            };
            actions.push(Arc::new(crate::InstallAction {
                id,
                name: inst.name.clone(),
                tags: inst.requires.clone(),
                depends: depends.clone(),
                check_cmd: inst.check_cmd.clone(),
                install_cmd: install_cmd.clone(),
                version: inst.version.clone(),
                variables: inst.variables.clone(),
            }));
        }
    }
    Ok(actions)
}

pub fn find_hermit_yaml_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == "hermit.yaml") {
        result.push(root.to_path_buf());
    } else if root.is_dir() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(find_hermit_yaml_files(&path));
                } else if path.file_name().is_some_and(|f| f == "hermit.yaml") {
                    result.push(path);
                }
            }
        }
    }
    result
}
