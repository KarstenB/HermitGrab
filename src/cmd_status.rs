use crate::{config::GlobalConfig, error, hermitgrab_error::StatusError, info, success};

pub(crate) fn get_status(cfg: &GlobalConfig) -> Result<(), StatusError> {
    for (dir, cfg) in &cfg.subconfigs {
        info!("Checking {dir}");
        for file in &cfg.files {
            let fs = file.check(cfg.path().parent().expect("Expected to get parent"), true);
            if fs.is_ok() {
                success!("Link {} is ok", file.source);
            } else {
                error!("Link {} reports a problem: {fs}", file.source);
            }
        }
    }
    Ok(())
}
