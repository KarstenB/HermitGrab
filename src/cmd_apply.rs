use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{load_hermit_config, Action, AtomicLinkAction, Cli, InstallAction, Tag};

pub fn apply(cli: Cli) -> Result<(), anyhow::Error> {
    let user_dirs = directories::UserDirs::new().expect("Could not get user directories");
    let search_root = user_dirs.home_dir().join(".hermitgrab");
    let yaml_files = crate::cmd_apply::find_hermit_yaml_files(&search_root);
    let mut configs = Vec::new();
    for path in &yaml_files {
        match load_hermit_config(path.to_str().unwrap()) {
            Ok(cfg) => configs.push((path.clone(), cfg)),
            Err(e) => eprintln!("[hermitgrab] Error loading {}: {}", path.display(), e),
        }
    }
    let user_tags: Vec<String> = cli.tags.iter().map(|t| t.to_string()).collect();
    let mut filtered_configs = Vec::new();
    for (path, cfg) in configs {
        if user_tags.is_empty() {
            filtered_configs.push((path, cfg));
        } else {
            // If config defines tags, require at least one positive match and no negative match
            let mut include = false;
            for tag in &cfg.tags {
                match tag {
                    Tag::Positive(t) => {
                        if user_tags.iter().any(|ut| ut == t) {
                            include = true;
                        }
                    }
                    Tag::Negative(t) => {
                        if user_tags.iter().any(|ut| ut == t) {
                            include = false;
                            break;
                        }
                    }
                }
            }
            if include || cfg.tags.is_empty() {
                filtered_configs.push((path, cfg));
            }
        }
    }
    let mut actions: Vec<Arc<dyn Action>> = Vec::new();
    for (path, cfg) in &filtered_configs {
        let depends = &cfg.depends;
        for file in &cfg.files {
            // Filter file actions by tags
            if !user_tags.is_empty() {
                let mut include = false;
                for tag in &file.tags {
                    match tag {
                        Tag::Positive(t) => {
                            if user_tags.iter().any(|ut| ut == t) {
                                include = true;
                            }
                        }
                        Tag::Negative(t) => {
                            if user_tags.iter().any(|ut| ut == t) {
                                include = false;
                                break;
                            }
                        }
                    }
                }
                if !include && !file.tags.is_empty() {
                    continue;
                }
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
                tags: file.tags.clone(),
                depends: depends.clone(),
            }));
        }
        for inst in &cfg.install {
            // Filter install actions by tags
            if !user_tags.is_empty() {
                let mut include = false;
                for tag in &inst.tags {
                    match tag {
                        Tag::Positive(t) => {
                            if user_tags.iter().any(|ut| ut == t) {
                                include = true;
                            }
                        }
                        Tag::Negative(t) => {
                            if user_tags.iter().any(|ut| ut == t) {
                                include = false;
                                break;
                            }
                        }
                    }
                }
                if !include && !inst.tags.is_empty() {
                    continue;
                }
            }
            let id = format!("install:{}:{}", path.display(), inst.name);
            let install_cmd = inst.source.as_ref().and_then(|src| cfg.sources.get(src));
            let Some(install_cmd) = install_cmd else {
                eprintln!("[hermitgrab] No source found for install: {}", inst.name);
                continue;
            };
            actions.push(Arc::new(InstallAction {
                id,
                name: inst.name.clone(),
                tags: inst.tags.clone(),
                depends: depends.clone(),
                check_cmd: inst.check_cmd.clone(),
                install_cmd: install_cmd.clone(),
                version: inst.version.clone(),
                variables: inst.variables.clone(),
            }));
        }
    }
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
