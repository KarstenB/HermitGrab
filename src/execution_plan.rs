use std::{
    collections::{BTreeSet, HashSet},
    str::FromStr,
    sync::Arc,
};

use serde::Serialize;

use crate::{
    RequireTag,
    action::{Action, ArcAction},
    config::{ArcHermitConfig, CliOptions, GlobalConfig, Tag},
    hermitgrab_error::{ActionError, ApplyError},
};
pub type ArcConfigAction = (ArcHermitConfig, ArcAction);
#[derive(Debug, Serialize)]
pub struct ExecutionPlan {
    pub actions: Vec<ArcConfigAction>,
}

pub struct ActionResult {
    pub action: ArcAction,
    pub result: Result<(), ActionError>,
}
impl ExecutionPlan {
    pub fn iter(&self) -> std::slice::Iter<'_, ArcConfigAction> {
        self.actions.iter()
    }
    pub fn filter_actions_by_tags(&self, active_tags: &BTreeSet<Tag>) -> ExecutionPlan {
        let mut filtered: Vec<ArcConfigAction> = Vec::new();
        for (cfg, action) in self.actions.iter() {
            let tags = action.requires();
            let mut matches = true;
            for tag in tags {
                if !tag.matches(active_tags) {
                    matches = false;
                    break;
                }
            }
            if matches {
                filtered.push((cfg.clone(), action.clone()));
            }
        }
        ExecutionPlan { actions: filtered }
    }

    pub fn sort_by_requires(&self) -> ExecutionPlan {
        use std::collections::HashMap;
        let mut sorted = Vec::new();
        let mut seen = HashSet::new();

        // Build a lookup map from Tag to ArcConfigAction for fast dependency resolution
        let mut tag_to_action: HashMap<Tag, &ArcConfigAction> = HashMap::new();
        for action in &self.actions {
            let (_, act) = action;
            for tag in act.provides() {
                tag_to_action.insert(tag.clone(), action);
            }
        }

        fn visit(
            a: &ArcConfigAction,
            tag_to_action: &HashMap<Tag, &ArcConfigAction>,
            seen: &mut HashSet<String>,
            sorted: &mut Vec<ArcConfigAction>,
        ) {
            let (cfg, a) = a;
            let id = a.id();
            if seen.contains(&id) {
                return;
            }
            seen.insert(id);
            for dep in a.requires() {
                let RequireTag::Positive(dep) = dep else {
                    // Skip negative dependencies
                    continue;
                };
                let tag = Tag::from_str(dep).unwrap();
                if let Some(dep_a) = tag_to_action.get(&tag) {
                    visit(dep_a, tag_to_action, seen, sorted);
                }
            }
            sorted.push((cfg.clone(), a.clone()));
        }
        for a in self.actions.iter() {
            visit(a, &tag_to_action, &mut seen, &mut sorted);
        }
        ExecutionPlan { actions: sorted }
    }

    pub fn execute_actions(&self) -> Vec<ActionResult> {
        let mut results = Vec::new();
        for (_, a) in self.actions.iter() {
            let res = a.execute();
            results.push(ActionResult {
                action: a.clone(),
                result: res,
            });
        }
        results
    }
}

impl<'a> IntoIterator for &'a ExecutionPlan {
    type Item = &'a ArcConfigAction;
    type IntoIter = std::slice::Iter<'a, ArcConfigAction>;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.iter()
    }
}

pub fn create_execution_plan(
    global_config: &Arc<GlobalConfig>,
    cli: &CliOptions,
) -> Result<ExecutionPlan, ApplyError> {
    let mut actions: Vec<(ArcHermitConfig, ArcAction)> = Vec::new();
    for (_, cfg) in global_config.subconfigs() {
        for item in cfg.config_items() {
            if let Ok(action) = item.as_action(cfg, cli) {
                actions.push((cfg.clone(), action));
            }
        }
    }
    Ok(ExecutionPlan { actions })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{
        HermitConfig, LinkConfig, LinkType,
        action::{Actions, install::InstallAction, link::LinkAction},
        config::FallbackOperation,
    };

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
        let cfg = Arc::new(cfg);
        let install_action = (cfg.clone(), install_action);
        let link_action_a = (cfg.clone(), link_action_a);
        let link_action_b = (cfg.clone(), link_action_b);
        let actions = vec![install_action, link_action_a, link_action_b];
        let plan = ExecutionPlan { actions };
        assert_eq!(plan.actions.len(), 3);
        let sorted_actions = plan.sort_by_requires();
        assert_eq!(sorted_actions.actions.len(), plan.actions.len());
        assert_eq!(
            sorted_actions.actions[0].1.id(),
            "hermitgrab::action::link::LinkAction:15529825494567548860"
        );
        assert_eq!(
            sorted_actions.actions[1].1.id(),
            "hermitgrab::action::link::LinkAction:16720280782580869565"
        );
        assert_eq!(
            sorted_actions.actions[2].1.id(),
            "hermitgrab::action::install::InstallAction:13896253663299332899"
        );
    }
}
