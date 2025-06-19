use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

use crate::{
    Action, InstallAction,
    config::{GlobalConfig, Tag},
    hermitgrab_error::{ActionError, ApplyError},
};

pub type ArcAction = Arc<dyn Action + 'static>;

pub struct ExecutionPlan {
    pub actions: Vec<ArcAction>,
}
impl ExecutionPlan {
    pub fn iter(&self) -> std::slice::Iter<'_, ArcAction> {
        self.actions.iter()
    }
    pub fn filter_actions_by_tags(&self, active_tags: &BTreeSet<Tag>) -> ExecutionPlan {
        let mut filtered: Vec<ArcAction> = Vec::new();
        for action in self.actions.iter() {
            let tags = action.tags();
            let mut matches = true;
            for tag in tags {
                if !tag.matches(active_tags) {
                    matches = false;
                    break;
                }
            }
            if matches {
                filtered.push(action.clone());
            }
        }
        ExecutionPlan { actions: filtered }
    }

    pub fn sort_by_dependency(&self) -> ExecutionPlan {
        let mut sorted = Vec::new();
        let mut seen = HashSet::new();
        fn visit(
            a: &ArcAction,
            actions: &ExecutionPlan,
            seen: &mut HashSet<String>,
            sorted: &mut Vec<ArcAction>,
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
        for a in self.actions.iter() {
            visit(a, self, &mut seen, &mut sorted);
        }
        ExecutionPlan {
            actions: sorted.into_iter().rev().collect(),
        }
    }

    pub fn execute_actions(&self) -> Vec<(String, Result<(), ActionError>)> {
        let mut results = Vec::new();
        for a in self.actions.iter() {
            let res = a.execute();
            results.push((a.short_description(), res));
        }
        results
    }
}

impl<'a> IntoIterator for &'a ExecutionPlan {
    type Item = &'a ArcAction;
    type IntoIter = std::slice::Iter<'a, ArcAction>;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.iter()
    }
}

pub fn create_execution_plan(global_config: &GlobalConfig) -> Result<ExecutionPlan, ApplyError> {
    let mut actions: Vec<Arc<dyn crate::Action>> = Vec::new();
    for (_, cfg) in &global_config.subconfigs {
        let depends = Vec::new();
        for file in &cfg.files {
            let id = format!("link:{}:{}", cfg.path().display(), file.target);
            let source = cfg
                .path()
                .parent()
                .expect("File should have a directory")
                .join(&file.source);
            actions.push(Arc::new(crate::LinkAction::new(
                id,
                &global_config.root_dir,
                source,
                file.target.clone(),
                file.get_requires(cfg),
                depends.clone(),
                file.link,
            )));
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
            actions.push(Arc::new(InstallAction::new(
                id,
                inst.name.clone(),
                inst.get_requires(cfg),
                depends.clone(),
                inst.check_cmd.clone(),
                inst.pre_install_cmd.clone(),
                inst.post_install_cmd.clone(),
                install_cmd.clone(),
                inst.version.clone(),
                inst.variables.clone(),
            )));
        }
    }
    Ok(ExecutionPlan { actions })
}
