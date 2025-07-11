use std::io::Write;
use std::sync::Arc;

use crossterm::style::{Attribute, Color, Stylize};

use crate::action::{Action, ArcAction};
use crate::commands::FallbackOperation;
use crate::common_cli::success;
use crate::common_cli::{stderr, stdout};
use crate::config::GlobalConfig;
use crate::execution_plan::{ExecutionPlan, create_execution_plan};
use crate::hermitgrab_error::{ActionError, ApplyError};
use crate::{error, hermitgrab_info};

#[allow(unused_imports)]
use crate::common_cli::step;

pub fn apply_with_tags(
    global_config: &Arc<GlobalConfig>,
    confirm: bool,
    verbose: bool,
    tags: &[String],
    profile: &Option<String>,
    fallback: &Option<FallbackOperation>,
) -> Result<(), ApplyError> {
    let active_tags = global_config.get_active_tags(tags, profile)?;
    let active_tags_str = active_tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    hermitgrab_info!("Active tags: {}", active_tags_str);
    let actions = create_execution_plan(global_config, fallback)?;
    let filtered_actions = actions.filter_actions_by_tags(&active_tags);
    let sorted = filtered_actions.sort_by_requires();
    present_execution_plan(&sorted);
    if !confirm {
        confirm_with_user()?;
    }
    let results = sorted.execute_actions();
    summarize(&sorted, &results, verbose);
    Ok(())
}

fn present_execution_plan(sorted: &ExecutionPlan) {
    hermitgrab_info("Execution plan:");
    for (i, a) in sorted.iter().enumerate() {
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

fn summarize(
    actions: &ExecutionPlan,
    results: &[(String, Result<(), ActionError>)],
    verbose: bool,
) {
    hermitgrab_info("Summary:");
    for (action, (desc, res)) in actions.iter().zip(results) {
        match res {
            Ok(_) => {
                success(desc);
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
