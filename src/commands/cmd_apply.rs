// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::{Arc, Mutex};

use crossterm::style::{Attribute, Color, Stylize};

use crate::action::{Action, ActionObserver, ActionOutput, ArcAction};
#[allow(unused_imports)]
use crate::common_cli::step;
use crate::common_cli::{stderr, stdout, success};
use crate::config::{CliOptions, GlobalConfig};
use crate::execution_plan::{ExecutionPlan, create_execution_plan};
use crate::hermitgrab_error::{ActionError, ApplyError};
use crate::{error, hermitgrab_info};

pub struct CliReporter {
    verbose: bool,
    reported_output: Mutex<BTreeMap<String, BTreeSet<String>>>,
    short_descriptions: Mutex<BTreeMap<String, String>>,
}
impl CliReporter {
    fn new(verbose: bool) -> Self {
        Self {
            verbose,
            reported_output: Mutex::new(BTreeMap::new()),
            short_descriptions: Mutex::new(BTreeMap::new()),
        }
    }
}

impl ActionObserver for CliReporter {
    fn action_started(&self, action: &ArcAction) {
        if !self.verbose {
            return;
        }
        hermitgrab_info!("Starting action: {}", action.short_description());
        let mut descriptions = self.short_descriptions.lock().expect("should lock");
        descriptions.insert(action.id().to_string(), action.short_description());
    }

    fn action_output(&self, action_id: &str, output: &ActionOutput) {
        if !self.verbose {
            return;
        }
        let mut map = self.reported_output.lock().expect("should lock");
        let descriptions = self.short_descriptions.lock().expect("should lock");
        let short_description = descriptions
            .get(action_id)
            .expect("Should have description");

        let reported_output = map.entry(action_id.to_string()).or_default();
        for (name, std_out, std_err) in output.clone() {
            if reported_output.contains(&name) {
                continue; // Skip already reported output
            }
            hermitgrab_info!("Output from: {}", short_description);
            if let Some(std_out) = std_out {
                stdout(&name, std_out.trim());
            }
            if let Some(std_err) = std_err {
                stderr(&name, std_err.trim());
            }
            reported_output.insert(name.to_string());
        }
    }

    fn action_progress(&self, action_id: &str, current: u64, total: u64, msg: &str) {
        if !self.verbose {
            return;
        }
        let descriptions = self.short_descriptions.lock().expect("should lock");
        let short_description = descriptions
            .get(action_id)
            .expect("Should have description");
        hermitgrab_info!(
            "Progress from: {} {} of {}: {}",
            short_description,
            current,
            total,
            msg
        );
    }

    fn action_finished(&self, action: &ArcAction, result: &Result<(), ActionError>) {
        let short_description = action.short_description();
        match result {
            Ok(_) => {
                success(&short_description);
                if self.verbose {
                    print_action_output(action);
                }
            }
            Err(e) => {
                error!("{}: {}", short_description, e);
                print_action_output(action);
            }
        }
    }
}

pub async fn apply_with_tags(
    global_config: &Arc<GlobalConfig>,
    cli: &CliOptions,
    parallel: bool,
) -> Result<(), ApplyError> {
    let active_tags = global_config.get_active_tags(&cli.tags, &cli.profile)?;
    let active_tags_str = active_tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    hermitgrab_info!("Active tags: {}", active_tags_str);
    let actions = create_execution_plan(global_config, cli)?;
    let filtered_actions = actions.filter_actions_by_tags(&active_tags);
    present_execution_plan(&filtered_actions, parallel);
    if !cli.confirm {
        confirm_with_user()?;
    }
    let observer = Arc::new(CliReporter::new(cli.verbose));
    let results = if !parallel {
        filtered_actions.execute_actions(&observer)
    } else {
        filtered_actions.execute_actions_parallel(&observer).await
    };
    if let Some(json_path) = &cli.json {
        let actions = filtered_actions
            .actions
            .iter()
            .map(|(_, action)| (action.id(), action))
            .collect::<BTreeMap<_, _>>();
        let results = results
            .iter()
            .map(|a| {
                let id = a.action.id();
                let output = a.action.get_output();
                (
                    id,
                    serde_json::json!({
                        "ok": a.result.is_ok(),
                        "error": a.result.as_ref().err().map(|e| e.to_string()),
                        "output": output,
                        "short_description": a.action.short_description(),
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let json = serde_json::json!({
            "actions": actions,
            "results": results,
        });
        std::fs::write(json_path, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(())
}

fn present_execution_plan(sorted: &ExecutionPlan, parallel: bool) {
    if parallel {
        hermitgrab_info!("Execution plan with parallel execution:");
    } else {
        hermitgrab_info!("Execution plan:");
    }
    for (i, (_, a)) in sorted.iter().enumerate() {
        crate::step!("[{:>2}] {}", i + 1, a.short_description());
    }
}

fn confirm_with_user() -> Result<(), ApplyError> {
    print!(
        "{} {}",
        "[hermitgrab]"
            .stylize()
            .with(Color::Cyan)
            .attribute(Attribute::Bold),
        "Do you want to apply the above actions? (y/n) "
            .stylize()
            .with(Color::Yellow)
    );
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    if !matches!(input.to_lowercase().trim(), "y" | "yes") {
        crate::common_cli::error("Aborted.");
        return Err(ApplyError::UserAborted);
    }
    Ok(())
}

fn print_action_output(action: &ArcAction) {
    if let Some(output) = action.get_output() {
        if output.is_empty() {
            return;
        }
        for (id, std_out, std_err) in output {
            if let Some(std_out) = std_out {
                stdout(&id, std_out.trim());
            }
            if let Some(std_err) = std_err {
                stderr(&id, std_err.trim());
            }
        }
    }
}
