//! annotations.json: the agent's prose and choices. It refers to facts by path or candidate id.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::facts::Evidence;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotations {
    pub version: u32,
    pub head: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub columns: Vec<Column>,
    #[serde(default)]
    pub files: BTreeMap<String, FileNote>,
    #[serde(default)]
    pub links: Vec<KeptLink>,
    #[serde(default)]
    pub dropped: Vec<Dropped>,
    #[serde(default)]
    pub context: Vec<ContextCard>,
    #[serde(default)]
    pub context_links: Vec<ContextLink>,
    #[serde(default)]
    pub missing: Vec<Missing>,
    pub trail: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileNote {
    pub what: String,
    #[serde(default)]
    pub outline: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptLink {
    pub candidate: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dropped {
    pub candidate: String,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextCard {
    pub path: String,
    pub what: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLink {
    pub from: String,
    pub to: String,
    pub reason: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Missing {
    pub id: String,
    pub name: String,
    pub why: String,
    pub near: String,
}

pub fn parse(text: &str) -> Result<Annotations, String> {
    serde_json::from_str(text).map_err(|e| format!("annotations.json: {e}"))
}
