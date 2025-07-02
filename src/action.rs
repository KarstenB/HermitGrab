use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use enum_dispatch::enum_dispatch;

use crate::RequireTag;
use crate::config::Tag;
use crate::hermitgrab_error::ActionError;
pub mod install;
pub mod link;
pub mod patch;

#[derive(Debug, Clone, Default)]
pub struct ActionOutput {
    output_order: Vec<String>,
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

    pub fn len(&self) -> usize {
        self.output_order.len()
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
#[enum_dispatch]
pub trait Action: Send + Sync + Hash {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn get_output(&self) -> Option<ActionOutput> {
        None
    }
    fn requires(&self) -> &[RequireTag];
    fn provides(&self) -> &[Tag];
    fn provides_tag(&self, tag: &Tag) -> bool {
        self.provides().iter().any(|t| t == tag)
    }
    fn id(&self) -> String {
        let mut hash = DefaultHasher::new();
        self.hash(&mut hash);
        format!("{}:{}", std::any::type_name::<Self>(), hash.finish())
    }
    fn execute(&self) -> Result<(), ActionError>;
}

#[enum_dispatch(Action)]
#[derive(Debug, Hash, PartialEq)]
pub enum Actions {
    Install(install::InstallAction),
    Link(link::LinkAction),
    Patch(patch::PatchAction),
}
pub type ArcAction = std::sync::Arc<Actions>;
