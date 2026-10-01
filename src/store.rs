//! Where maps live: `<root>/<repo>/<branch with / → __>/<head-sha>/{facts,annotations}.json`.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use crate::facts::{self, Facts};

pub fn state_root() -> PathBuf {
    if let Some(d) = env::var_os("CODEMAPX_STATE_DIR") {
        return d.into();
    }
    if let Some(d) = env::var_os("XDG_STATE_HOME") {
        return Path::new(&d).join("codemapx");
    }
    PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state/codemapx")
}

pub fn branch_dir(root: &Path, repo: &str, branch: &str) -> PathBuf {
    root.join(repo).join(branch.replace('/', "__"))
}

/// Writes facts.json; a new head dir inherits the newest earlier annotations.json for the agent to update.
pub fn save_facts(root: &Path, facts: &Facts) -> io::Result<PathBuf> {
    let bdir = branch_dir(root, &facts.repo, &facts.branch);
    let prev = newest_map(&bdir);
    let dir = bdir.join(&facts.head);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("facts.json"), facts::to_json(facts))?;
    let ann = dir.join("annotations.json");
    if !ann.exists() {
        if let Some(src) = prev.filter(|p| *p != dir).map(|p| p.join("annotations.json")).filter(|p| p.exists()) {
            fs::copy(src, &ann)?;
        }
    }
    Ok(dir)
}

pub fn newest_map(branch_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(branch_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| Some((fs::metadata(p.join("facts.json")).ok()?.modified().ok()?, p)))
        .max()
        .map(|(_, p)| p)
}

pub fn find_map(root: &Path, repo: &str, branch: &str, head: &str) -> Option<PathBuf> {
    let b = branch_dir(root, repo, branch);
    let exact = b.join(head);
    if exact.join("facts.json").exists() { Some(exact) } else { newest_map(&b) }
}
