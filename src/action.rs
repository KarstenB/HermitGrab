// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use serde::Serialize;
use xxhash_rust::xxh3::Xxh3;

use crate::hermitgrab_error::ActionError;
use crate::{HermitConfig, RequireTag};
pub mod install;
pub mod link;
pub mod patch;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ActionOutput {
    pub output_order: Vec<String>,
    standard_output: HashMap<String, String>,
    error_output: HashMap<String, String>,
}

impl ActionOutput {
    pub fn new_stdout(stdout: String) -> Self {
        let mut output = Self::default();
        output.standard_output.insert("stdout".to_string(), stdout);
        output.output_order.push("stdout".to_string());
        output
    }

    fn add(&mut self, name: &str, stdout: &str, stderr: &str) {
        if !stdout.is_empty() {
            self.standard_output
                .insert(name.to_string(), stdout.to_string());
            if !self.output_order.contains(&name.to_string()) {
                self.output_order.push(name.to_string());
            }
        }
        if !stderr.is_empty() {
            self.error_output
                .insert(name.to_string(), stderr.to_string());
            if !self.output_order.contains(&name.to_string()) {
                self.output_order.push(name.to_string());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.output_order.is_empty()
    }
}

impl IntoIterator for ActionOutput {
    type Item = (String, Option<String>, Option<String>);
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.output_order.into_iter().map(move |key| {
            (
                key.clone(),
                self.standard_output.get(&key).cloned(),
                self.error_output.get(&key).cloned(),
            )
        }))
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Status {
    Ok(String),
    NotOk(String),
    Error(String),
    NotSupported,
}

pub trait ActionObserver {
    fn action_started(&self, action: &ArcAction);
    fn action_output(&self, action_id: &str, output: &ActionOutput);
    fn action_progress(&self, action_id: &str, current: u64, total: u64, msg: &str);
    fn action_finished(&self, action: &ArcAction, result: &Result<(), ActionError>);
}

#[enum_dispatch]
pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn get_output(&self) -> Option<ActionOutput> {
        None
    }
    fn requires(&self) -> &[RequireTag];
    fn id(&self) -> String;
    fn execute(&self, observer: &Arc<impl ActionObserver>) -> Result<(), ActionError>;
    fn get_status(&self, cfg: &HermitConfig, quick: bool) -> Status;
    fn get_order(&self) -> u64;
}

pub fn id_from_hash<T: Hash>(item: &T) -> String {
    let mut hash = Xxh3::new();
    item.hash(&mut hash);
    format!("{}:{:016x}", std::any::type_name::<T>(), hash.finish())
}

#[enum_dispatch(Action)]
#[derive(Debug, Hash, PartialEq, Serialize)]
pub enum Actions {
    Install(install::InstallAction),
    Link(link::LinkAction),
    Patch(patch::PatchAction),
}
pub type ArcAction = std::sync::Arc<Actions>;
