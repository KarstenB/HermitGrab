use std::{
    collections::{BTreeSet, HashSet},
    str::FromStr,
    sync::Arc,
};

use crate::{
    RequireTag,
    action::{
        Action, Actions, ArcAction, install::InstallAction, link::LinkAction, patch::PatchAction,
    },
    config::{FallbackOperation, GlobalConfig, Tag},
    hermitgrab_error::{ActionError, ApplyError},
};
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

pub fn create_execution_plan(
    global_config: &Arc<GlobalConfig>,
    fallback: &Option<FallbackOperation>,
) -> Result<ExecutionPlan, ApplyError> {
    let mut actions: Vec<ArcAction> = Vec::new();
    for (_, cfg) in global_config.subconfigs() {
        for link_config in &cfg.link {
            actions.push(Arc::new(Actions::Link(LinkAction::new(
                link_config,
                cfg,
                fallback,
            ))));
        }
        for patch in &cfg.patch {
            actions.push(Arc::new(Actions::Patch(PatchAction::new(patch, cfg))));
        }
        for install_entry in &cfg.install {
            actions.push(Arc::new(Actions::Install(InstallAction::new(
                install_entry,
                cfg,
            )?)));
        }
    }
    Ok(ExecutionPlan { actions })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{HermitConfig, LinkConfig, LinkType, config::FallbackOperation};

    #[test]
    fn test_topology_sorting() {
        let default_config = Arc::new(GlobalConfig::default());
        let mut cfg = HermitConfig::create_new(
            &PathBuf::from("/tmp/hermitgrab"),
            Arc::downgrade(&default_config),
        );
        cfg.sources
            .insert("install_source".to_string(), "install_cmd".to_string());
        let link_action_a = Arc::new(Actions::Link(LinkAction::new(
            &LinkConfig {
                source: PathBuf::from("/source/a"),
                target: PathBuf::from("target_a"),
                link: LinkType::Soft,
                requires: BTreeSet::from_iter(vec![RequireTag::Positive("tag_a".to_string())]),
                provides: BTreeSet::from_iter(vec![Tag::from_str("tag_a").unwrap()]),
                fallback: FallbackOperation::Abort,
            },
            &cfg,
            &None,
        )));
        let link_action_b = Arc::new(Actions::Link(LinkAction::new(
            &LinkConfig {
                source: PathBuf::from("/source/b"),
                target: PathBuf::from("target_b"),
                link: LinkType::Soft,
                requires: BTreeSet::from_iter(vec![RequireTag::Positive("tag_a".to_string())]),
                provides: BTreeSet::from_iter(vec![Tag::from_str("tag_b").unwrap()]),
                fallback: FallbackOperation::Abort,
            },
            &cfg,
            &None,
        )));
        let install_action = Arc::new(Actions::Install(
            InstallAction::new(
                &crate::InstallConfig {
                    name: "action".to_string(),
                    source: "install_source".to_string(),
                    version: Some("1.0.0".to_string()),
                    requires: BTreeSet::from_iter(vec![RequireTag::Positive("tag_b".to_string())]),
                    ..Default::default()
                },
                &cfg,
            )
            .unwrap(),
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
