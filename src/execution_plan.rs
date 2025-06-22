use std::{
    collections::{BTreeSet, HashSet},
    str::FromStr,
    sync::Arc,
};

use crate::{
    Action, InstallAction, LinkAction, RequireTag,
    action::PatchAction,
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
            let tags = action.requires();
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

    pub fn sort_by_requires(&self) -> ExecutionPlan {
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
            seen.insert(a.id());
            for dep in a.requires() {
                let RequireTag::Positive(dep) = dep else {
                    // Skip negative dependencies
                    continue;
                };
                let tag = Tag::from_str(dep).unwrap();
                if let Some(dep_a) = actions.iter().find(|x| x.provides_tag(&tag)) {
                    visit(dep_a, actions, seen, sorted);
                }
            }
            sorted.push(a.clone());
        }
        for a in self.actions.iter() {
            visit(a, self, &mut seen, &mut sorted);
        }
        ExecutionPlan { actions: sorted }
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
    for cfg in global_config.subconfigs.values() {
        for file in &cfg.file {
            let id = format!("link:{}:{}", cfg.path().display(), file.target);
            let source = cfg
                .path()
                .parent()
                .expect("File should have a directory")
                .join(&file.source);
            actions.push(Arc::new(LinkAction::new(
                id,
                &global_config.root_dir,
                source,
                file.target.clone(),
                file.get_requires(cfg),
                cfg.provides.clone(),
                file.link,
            )));
        }
        for patch in &cfg.patch {
            let id = format!("link:{}:{}", cfg.path().display(), patch.target);
            let source = cfg
                .path()
                .parent()
                .expect("File should have a directory")
                .join(&patch.source);
            actions.push(Arc::new(PatchAction::new(
                id,
                &global_config.root_dir,
                source,
                patch.target.clone(),
                patch.get_requires(cfg),
                cfg.provides.clone(),
                patch.patch_type.clone(),
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
            let mut variables = inst.variables.clone();
            variables.insert(
                "hermit.root_dir".to_string(),
                global_config.root_dir.to_string_lossy().to_string(),
            );
            variables.insert(
                "hermit.this_dir".to_string(),
                cfg.path().to_string_lossy().to_string(),
            );
            actions.push(Arc::new(InstallAction::new(
                id,
                inst.name.clone(),
                inst.get_requires(cfg),
                cfg.provides.clone(),
                inst.check_cmd.clone(),
                inst.pre_install_cmd.clone(),
                inst.post_install_cmd.clone(),
                install_cmd.clone(),
                inst.version.clone(),
                variables,
            )));
        }
    }
    Ok(ExecutionPlan { actions })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::GlobalConfig;

    #[test]
    fn test_create_execution_plan() {
        let global_config = GlobalConfig::default();
        let plan = create_execution_plan(&global_config);
        assert!(plan.is_ok());
        let plan = plan.unwrap();
        assert!(!plan.actions.is_empty());
    }

    #[test]
    fn test_topology_sorting() {
        let link_action_a = Arc::new(crate::LinkAction::new(
            "link:action_a".to_string(),
            &PathBuf::from("/tmp/hermitgrab"),
            PathBuf::from("/source/a"),
            "target_a".to_string(),
            BTreeSet::new(),
            BTreeSet::from_iter(vec![Tag::from_str("tag_a").unwrap()]),
            crate::LinkType::Soft,
        ));
        let link_action_b = Arc::new(crate::LinkAction::new(
            "link:action_b".to_string(),
            &PathBuf::from("/tmp/hermitgrab"),
            "/source/b".into(),
            "target_b".to_string(),
            BTreeSet::from_iter(vec![RequireTag::Positive("tag_a".to_string())]),
            BTreeSet::from_iter(vec![Tag::from_str("tag_b").unwrap()]),
            crate::LinkType::Soft,
        ));
        let install_action = Arc::new(crate::InstallAction::new(
            "install:action".to_string(),
            "install_action".to_string(),
            BTreeSet::from_iter(vec![RequireTag::Positive("tag_b".to_string())]),
            BTreeSet::from_iter(vec![Tag::from_str("tag_install").unwrap()]),
            None,
            None,
            None,
            "install_cmd".to_string(),
            None,
            std::collections::BTreeMap::new(),
        ));
        let actions: Vec<ArcAction> = vec![install_action, link_action_a, link_action_b];
        let plan = ExecutionPlan { actions };
        assert_eq!(plan.actions.len(), 3);
        let sorted_actions = plan.sort_by_requires();
        assert_eq!(sorted_actions.actions.len(), plan.actions.len());
        assert_eq!(sorted_actions.actions[0].id(), "link:action_a");
        assert_eq!(sorted_actions.actions[1].id(), "link:action_b");
        assert_eq!(sorted_actions.actions[2].id(), "install:action");
    }
}
