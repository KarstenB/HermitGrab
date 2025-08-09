// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::Serialize;
use tokio::task::JoinSet;

use crate::{
    action::{Action, ActionObserver, ArcAction},
    config::{ArcHermitConfig, CliOptions, GlobalConfig, Tag},
    hermitgrab_error::{ActionError, ApplyError, ConfigError::HermitConfigNotAction},
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

    pub fn execute_actions(&self, observer: &Arc<impl ActionObserver>) -> Vec<ActionResult> {
        let mut results = Vec::new();
        for (_, a) in self.actions.iter() {
            observer.action_started(a);
            let res = a.execute(observer);
            observer.action_finished(a, &res);
            results.push(ActionResult {
                action: a.clone(),
                result: res,
            });
        }
        results
    }

    pub async fn execute_actions_parallel(
        &self,
        observer: &Arc<impl ActionObserver + Sync + Send + 'static>,
    ) -> Vec<ActionResult> {
        let mut actions_by_order = BTreeMap::new();
        for (_, a) in self.actions.iter() {
            let order = a.get_order();
            actions_by_order
                .entry(order)
                .or_insert_with(Vec::new)
                .push(a.clone());
        }
        let mut results = Vec::new();
        for (_, actions) in actions_by_order {
            let mut tasks = JoinSet::new();
            for action in actions {
                let observer = observer.clone();
                tasks.spawn(async move {
                    observer.action_started(&action);
                    let result = action.execute(&observer);
                    observer.action_finished(&action, &result);
                    ActionResult { action, result }
                });
            }
            while let Some(res) = tasks.join_next().await {
                match res {
                    Ok(action_result) => results.push(action_result),
                    Err(e) => {
                        crate::error!("Error executing action: {e}");
                    }
                }
            }
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
    for (path, cfg) in global_config.subconfigs() {
        for item in cfg.config_items() {
            match item.as_action(cfg, cli) {
                Ok(action) => {
                    actions.push((cfg.clone(), action));
                }
                Err(e) => match e {
                    HermitConfigNotAction => {}
                    e => {
                        crate::error!(
                            "An error occured when preparing action in {path} for {}: {e}",
                            item.id()
                        )
                    }
                },
            }
        }
    }
    actions.sort_by_key(|(_, action)| action.get_order());
    Ok(ExecutionPlan { actions })
}
