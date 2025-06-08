use std::io::Write;
use std::sync::Arc;

use crossterm::style::{Attribute, Color, Stylize};

use crate::common_cli::{self, error, hermitgrab_info, success};
use crate::config::GlobalConfig;
use crate::execution_plan::{ExecutionPlan, create_execution_plan};
use crate::hermitgrab_error::{ActionError, ApplyError};
use crate::{Action, Cli};

pub(crate) fn apply_with_tags(cli: Cli, global_config: &GlobalConfig) -> Result<(), ApplyError> {
    let active_tags = global_config.get_active_tags(&cli.tags, &cli.profile)?;
    let active_tags_str = active_tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    hermitgrab_info(&format!("Active tags: {}", active_tags_str));
    let actions = create_execution_plan(global_config)?;
    let filtered_actions = actions.filter_actions_by_tags(&active_tags);
    let sorted = filtered_actions.sort_by_dependency();
    present_execution_plan(&sorted);
    if !cli.confirm {
        confirm_with_user()?;
    }
    let results = sorted.execute_actions();
    summarize(&sorted, &results, cli.verbose);
    Ok(())
}

fn present_execution_plan(sorted: &ExecutionPlan) {
    hermitgrab_info("Execution plan:");
    for a in sorted.iter() {
        hermitgrab_info(&format!("  [ ] {}", a.short_description()));
    }
}

fn confirm_with_user() -> Result<(), ApplyError> {
    print!(
        "{} Proceed? [y/N]: ",
        "[hermitgrab]"
            .stylize()
            .with(Color::Cyan)
            .attribute(Attribute::Bold)
    );
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    if !matches!(input.to_lowercase().trim(), "y" | "yes") {
        error("Aborted.");
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
                error(&format!("{}: {}", desc, e));
                print_action_output(action);
            }
        }
    }
}

fn print_action_output(action: &Arc<dyn Action>) {
    if let Some(output) = action.get_output() {
        let stdout = output.standard_output().trim();
        let stderr = output.error_output().trim();
        if !stdout.is_empty() {
            common_cli::stdout(stdout);
        }
        if !stderr.is_empty() {
            common_cli::stderr(stderr);
        }
    }
}
