// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeSet, sync::Arc};

use serde::Serialize;

use crate::{
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
    for (path, cfg) in global_config.subconfigs() {
        for item in cfg.config_items() {
            match item.as_action(cfg, cli) {
                Ok(action) => {
                    actions.push((cfg.clone(), action));
                }
                Err(e) => match e {
                    crate::hermitgrab_error::ConfigError::HermitConfigNotAction => {}
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
    Ok(ExecutionPlan { actions })
}
