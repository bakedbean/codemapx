//! Where maps live: `<root>/<repo>/<branch with / → __>/<head-sha>/{facts,annotations}.json`.
//! Lookups also search the dirs of the branch's former names, so a renamed branch keeps its maps.

use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::facts::{self, Facts};

pub fn state_root() -> PathBuf {
    state_root_from(env::var_os("CODEMAPX_STATE_DIR"), env::var_os("XDG_STATE_HOME"), env::var_os("HOME"))
}

/// Empty values count as unset.
pub fn state_root_from(codemapx: Option<OsString>, xdg: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty());
    if let Some(d) = set(codemapx) {
        return d.into();
    }
    if let Some(d) = set(xdg) {
        return Path::new(&d).join("codemapx");
    }
    PathBuf::from(home.unwrap_or_default()).join(".local/state/codemapx")
}

pub fn branch_dir(root: &Path, repo: &str, branch: &str) -> PathBuf {
    root.join(repo).join(branch.replace('/', "__"))
}

/// A name the branch had until `until`, when it was renamed away. Maps saved under it later belong to
/// whichever branch reused the name.
#[derive(Debug, Clone, PartialEq)]
pub struct Former {
    pub name: String,
    pub until: SystemTime,
}

/// Writes facts.json; a new head dir inherits the newest earlier annotations.json for the agent to update,
/// with candidate ids remapped by (from, to) since ids are positional. Vanished pairs become `gone:<from>-><to>`.
pub fn save_facts(root: &Path, facts: &Facts, former: &[Former]) -> io::Result<PathBuf> {
    let dir = branch_dir(root, &facts.repo, &facts.branch).join(&facts.head);
    let maps = maps(root, &facts.repo, &facts.branch, former);
    let prev = newest(&maps, |p| *p != dir && p.join("annotations.json").exists());
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("facts.json"), facts::to_json(facts))?;
    let ann = dir.join("annotations.json");
    if let Some(prev) = prev.filter(|_| !ann.exists()) {
        let text = fs::read_to_string(prev.join("annotations.json"))?;
        fs::write(&ann, carry(&prev, &text, facts).unwrap_or(text))?;
    }
    Ok(dir)
}

// Map dirs with their facts.json mtimes: the current name's first, then each former name's from before its rename.
fn maps(root: &Path, repo: &str, branch: &str, former: &[Former]) -> Vec<(SystemTime, PathBuf)> {
    let mut sources: Vec<(&str, Option<SystemTime>)> = vec![(branch, None)];
    for f in former {
        match sources.iter_mut().find(|(n, _)| *n == f.name) {
            Some((_, until)) => *until = (*until).map(|u| u.max(f.until)),
            None => sources.push((&f.name, Some(f.until))),
        }
    }
    sources
        .into_iter()
        .flat_map(|(name, until)| {
            fs::read_dir(branch_dir(root, repo, name))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter_map(|p| Some((fs::metadata(p.join("facts.json")).ok()?.modified().ok()?, p)))
                .filter(move |(t, _)| until.is_none_or(|u| *t <= u))
        })
        .collect()
}

// None when nothing needs rewriting (or the old files don't parse), so the text is copied as is.
fn carry(prev: &Path, text: &str, facts: &Facts) -> Option<String> {
    let old: Facts = serde_json::from_str(&fs::read_to_string(prev.join("facts.json")).ok()?).ok()?;
    let mut ann: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut changed = false;
    for key in ["links", "dropped"] {
        for item in ann.get_mut(key).and_then(|v| v.as_array_mut()).into_iter().flatten() {
            let Some(id) = item.get("candidate").and_then(|v| v.as_str()) else { continue };
            let Some(c) = old.candidates.iter().find(|c| c.id == id) else { continue };
            let new_id = match facts.candidates.iter().find(|n| n.from == c.from && n.to == c.to) {
                Some(n) => n.id.clone(),
                None => format!("gone:{}->{}", c.from, c.to),
            };
            if new_id != id {
                item["candidate"] = new_id.into();
                changed = true;
            }
        }
    }
    changed.then(|| serde_json::to_string_pretty(&ann).ok().map(|s| s + "\n")).flatten()
}

// Newest map dir (by facts.json mtime) that `keep` accepts.
fn newest(maps: &[(SystemTime, PathBuf)], keep: impl Fn(&PathBuf) -> bool) -> Option<PathBuf> {
    maps.iter().filter(|(_, p)| keep(p)).max().map(|(_, p)| p.clone())
}

fn is_head(p: &Path, head: &str) -> bool {
    p.file_name().is_some_and(|n| n == head)
}

/// The map view/html show: prefer a finished map (annotations written for its own head), then any
/// annotated map, exact HEAD first each time; else the exact/newest dir so load reports what's missing.
pub fn find_map(root: &Path, repo: &str, branch: &str, former: &[Former], head: &str) -> Option<PathBuf> {
    let maps = maps(root, repo, branch, former);
    for want in [Ann::Finished, Ann::Carried] {
        let found = newest(&maps, |p| is_head(p, head) && ann_state(p) == want).or_else(|| newest(&maps, |p| ann_state(p) == want));
        if found.is_some() {
            return found;
        }
    }
    maps.iter().find(|(_, p)| is_head(p, head)).map(|(_, p)| p.clone()).or_else(|| newest(&maps, |_| true))
}

/// validate checks the exact-HEAD map when there is one, so the agent's loop sees its own edits.
pub fn find_map_for_validate(root: &Path, repo: &str, branch: &str, former: &[Former], head: &str) -> Option<PathBuf> {
    let maps = maps(root, repo, branch, former);
    maps.iter().find(|(_, p)| is_head(p, head)).map(|(_, p)| p.clone()).or_else(|| find_map(root, repo, branch, former, head))
}

#[derive(PartialEq)]
enum Ann {
    None,
    Carried,
    Finished,
}

// A dir is named for its facts' head, so annotations for that head are finished.
fn ann_state(dir: &Path) -> Ann {
    let Ok(text) = fs::read_to_string(dir.join("annotations.json")) else { return Ann::None };
    let head = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v["head"].as_str().map(String::from));
    if head.as_deref().is_some_and(|h| dir.file_name().is_some_and(|n| n == h)) { Ann::Finished } else { Ann::Carried }
}
