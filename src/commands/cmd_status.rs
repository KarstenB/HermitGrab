use std::sync::Arc;

use crate::{config::GlobalConfig, error, hermitgrab_error::StatusError, info, success};

pub fn get_status(cfg: &Arc<GlobalConfig>) -> Result<(), StatusError> {
    for (dir, cfg) in cfg.subconfigs() {
        info!("Checking {dir}");
        for file in &cfg.file {
            let fs = file.check(cfg, true);
            if fs.is_ok() {
                success!("Link {} is ok", file.source.display());
            } else {
                error!("Link {} reports a problem: {fs}", file.source.display());
            }
        }
    }
    Ok(())
}
