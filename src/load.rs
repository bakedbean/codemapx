//! Finds and reads the map for a worktree's current branch.

use std::{fs, path::PathBuf};

use crate::{
    annotations::{self, Annotations},
    facts::Facts,
    git::Git,
    map::{self, Map},
    store,
};

pub const NO_MAP: &str = "run /codemapx in the agent session (or codemapx collect, then write annotations.json)";

pub struct Loaded {
    pub dir: PathBuf,
    pub facts: Facts,
    pub annotations: Annotations,
}

/// `for_validate` picks the exact-HEAD map when there is one; otherwise the newest finished map wins.
pub fn load(git: &Git, for_validate: bool) -> Result<Loaded, String> {
    let (repo, branch, head) = (git.repo_name()?, git.branch()?, git.head()?);
    let find = if for_validate { store::find_map_for_validate } else { store::find_map };
    let dir = find(&store::state_root(), &repo, &branch, &head).ok_or_else(|| format!("no map for {branch}; {NO_MAP}"))?;
    let facts_text = fs::read_to_string(dir.join("facts.json")).map_err(|e| format!("{}: {e}", dir.join("facts.json").display()))?;
    let facts: Facts = serde_json::from_str(&facts_text).map_err(|e| format!("facts.json: {e}"))?;
    let text = fs::read_to_string(dir.join("annotations.json")).map_err(|_| format!("{}: no annotations.json; {NO_MAP}", dir.display()))?;
    Ok(Loaded { annotations: annotations::parse(&text)?, dir, facts })
}

/// Evidence is checked against the facts' HEAD, not the worktree.
pub fn merge(git: &Git, l: &Loaded) -> Result<Map, Vec<String>> {
    map::merge(&l.facts, &l.annotations, &|p| git.show(&l.facts.head, p))
}
