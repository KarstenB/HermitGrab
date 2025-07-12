use std::{collections::BTreeMap, sync::Arc};

use crate::{
    action::{Action, Status},
    config::{CliOptions, GlobalConfig},
    error,
    execution_plan::create_execution_plan,
    hermitgrab_error::StatusError,
    hermitgrab_info, success, warn,
};

pub fn get_status(
    global_config: &Arc<GlobalConfig>,
    quick: bool,
    cli: &CliOptions,
) -> Result<(), StatusError> {
    let active_tags = global_config.get_active_tags(&cli.tags, &cli.profile)?;
    let active_tags_str = active_tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    hermitgrab_info!("Active tags: {}", active_tags_str);
    let actions = create_execution_plan(global_config, cli)?;
    let filtered_actions = actions.filter_actions_by_tags(&active_tags);
    let mut results = Vec::new();
    for (cfg, action) in filtered_actions.iter() {
        let fs = action.get_status(cfg, quick);
        match &fs {
            Status::Ok(msg) => success!("{}", msg),
            Status::NotOk(msg) => warn!("{}", msg),
            Status::Error(msg) => error!("{}", msg),
            Status::NotSupported => {}
        }
        results.push((action.id(), fs));
    }
    if let Some(json_path) = &cli.json {
        let actions = filtered_actions
            .actions
            .iter()
            .map(|(_, action)| (action.id(), action))
            .collect::<BTreeMap<_, _>>();
        let results = results.into_iter().collect::<BTreeMap<_, _>>();
        let json = serde_json::json!({
            "actions": actions,
            "results": results,
        });
        std::fs::write(json_path, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(())
}
