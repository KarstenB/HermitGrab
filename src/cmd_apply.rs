use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::GlobalConfig;
use crate::execution_plan::{ExecutionPlan, create_execution_plan};
use crate::hermitgrab_error::{ActionError, ApplyError};
use crate::{Action, Cli};

pub(crate) fn apply_with_tags(
    cli: Cli,
    global_config: &GlobalConfig,
) -> Result<(), ApplyError> {
    let active_tags = global_config.get_active_tags(&cli.tags, &cli.profile)?;
    println!("[hermitgrab] Active tags: {:?}", active_tags);
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
    println!("[hermitgrab] Execution plan:");
    for (i, a) in sorted.iter().enumerate() {
        println!(
            "{}. {} [tags: {:?}]",
            i + 1,
            a.short_description(),
            a.tags()
        );
    }
}

fn confirm_with_user() -> Result<(), ApplyError> {
    print!("Proceed? [y/N]: ");
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    if !matches!(input.to_lowercase().trim(), "y" | "yes") {
        println!("Aborted.");
        return Err(ApplyError::UserAborted);
    }
    Ok(())
}

fn summarize(
    actions: &ExecutionPlan,
    results: &[(String, Result<(), ActionError>)],
    verbose: bool,
) {
    println!("[hermitgrab] Summary:");
    for (action, (desc, res)) in actions.iter().zip(results) {
        match res {
            Ok(_) => {
                println!("[ok] {}", desc);
                if verbose {
                    print_action_output(action);
                }
            }
            Err(e) => {
                println!("[err] {}: {}", desc, e);
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
            println!("[stdout] {}", stdout);
        }
        if !stderr.is_empty() {
            eprintln!("[stderr] {}", stderr);
        }
    }
}

pub fn find_hermit_yaml_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == "hermit.yaml") {
        result.push(root.to_path_buf());
    } else if root.is_dir() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(find_hermit_yaml_files(&path));
                } else if path.file_name().is_some_and(|f| f == "hermit.yaml") {
                    result.push(path);
                }
            }
        }
    }
    result
}
