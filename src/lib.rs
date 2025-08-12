// SPDX-FileCopyrightText: 2025 Karsten Becker
// SPDX-License-Identifier: GPL-3.0-only

pub mod action;
pub mod build_doc;
pub mod commands;
pub mod common_cli;
pub mod config;
pub mod detector;
pub mod execution_plan;
pub mod file_ops;
pub mod hermitgrab_error;
pub mod integrations;

// Re-export key types for compatibility with main.rs and all modules
pub use config::{HermitConfig, InstallConfig, LinkConfig, LinkType, RequireTag};
pub use hermitgrab_error::{
    AddError, ApplyError, ConfigError, DiscoverError, FileOpsError, StatusError,
};
