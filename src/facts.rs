//! facts.json: what `collect` found. Only code writes it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub version: u32,
    pub repo: String,
    pub branch: String,
    pub branch_issue: Option<String>,
    pub base: String,
    pub head: String,
    pub commits: Vec<Commit>,
    pub files: Vec<FileFacts>,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileFacts {
    pub path: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub binary: bool,
    pub add: u32,
    pub del: u32,
    pub diff: String,
    pub outline: Vec<OutlineItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineItem {
    pub name: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
}

/// Declaration order is strength: dedup keeps the greatest kind per (from, to).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateKind {
    TestPair,
    Import,
    ExportUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: CandidateKind,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    pub line: usize,
    pub quote: String,
}

pub fn to_json(facts: &Facts) -> String {
    let mut s = serde_json::to_string_pretty(facts).expect("facts serialize");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_kebab_and_order_by_strength() {
        assert_eq!(serde_json::to_string(&CandidateKind::ExportUse).unwrap(), "\"export-use\"");
        assert!(CandidateKind::ExportUse > CandidateKind::Import && CandidateKind::Import > CandidateKind::TestPair);
    }
}
