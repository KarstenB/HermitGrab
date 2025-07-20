// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use crossterm::style::{Attribute, Color, Stylize};

use crate::action::{Action, ArcAction};
use crate::common_cli::success;
use crate::common_cli::{stderr, stdout};
use crate::config::{CliOptions, GlobalConfig};
use crate::execution_plan::{ActionResult, ExecutionPlan, create_execution_plan};
use crate::hermitgrab_error::ApplyError;
use crate::{error, hermitgrab_info};

#[allow(unused_imports)]
use crate::common_cli::step;

pub fn apply_with_tags(
    global_config: &Arc<GlobalConfig>,
    cli: &CliOptions,
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
    present_execution_plan(&filtered_actions);
    if !cli.confirm {
        confirm_with_user()?;
    }
    let results = filtered_actions.execute_actions();
    summarize(&results, cli.verbose);
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

fn present_execution_plan(sorted: &ExecutionPlan) {
    hermitgrab_info("Execution plan:");
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

fn summarize(results: &[ActionResult], verbose: bool) {
    hermitgrab_info("Summary:");
    for result in results {
        let action = &result.action;
        let desc = action.short_description();
        let res = &result.result;
        match res {
            Ok(_) => {
                success(&desc);
                if verbose {
                    print_action_output(action);
                }
            }
            Err(e) => {
                error!("{}: {}", desc, e);
                print_action_output(action);
            }
        }
    }
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
