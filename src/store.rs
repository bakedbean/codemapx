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

/// Writes facts.json; a new head dir inherits the newest earlier annotations.json for the agent to update,
/// with candidate ids remapped by (from, to) since ids are positional. Vanished pairs become `gone:<from>-><to>`.
pub fn save_facts(root: &Path, facts: &Facts) -> io::Result<PathBuf> {
    let bdir = branch_dir(root, &facts.repo, &facts.branch);
    let dir = bdir.join(&facts.head);
    let prev = newest(&bdir, |p| *p != dir && p.join("annotations.json").exists());
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("facts.json"), facts::to_json(facts))?;
    let ann = dir.join("annotations.json");
    if let Some(prev) = prev.filter(|_| !ann.exists()) {
        let text = fs::read_to_string(prev.join("annotations.json"))?;
        fs::write(&ann, carry(&prev, &text, facts).unwrap_or(text))?;
    }
    Ok(dir)
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

pub fn newest_map(branch_dir: &Path) -> Option<PathBuf> {
    newest(branch_dir, |_| true)
}

// Newest map dir (by facts.json mtime) that `keep` accepts.
fn newest(branch_dir: &Path, keep: impl Fn(&PathBuf) -> bool) -> Option<PathBuf> {
    fs::read_dir(branch_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| keep(p))
        .filter_map(|p| Some((fs::metadata(p.join("facts.json")).ok()?.modified().ok()?, p)))
        .max()
        .map(|(_, p)| p)
}

/// The map view/html show: prefer a finished map (annotations written for its own head), then any
/// annotated map, exact HEAD first each time; else the exact/newest dir so load reports what's missing.
pub fn find_map(root: &Path, repo: &str, branch: &str, head: &str) -> Option<PathBuf> {
    let b = branch_dir(root, repo, branch);
    let exact = Some(b.join(head)).filter(|e| e.join("facts.json").exists());
    for want in [Ann::Finished, Ann::Carried] {
        if let Some(e) = exact.as_ref().filter(|e| ann_state(e) == want) {
            return Some(e.clone());
        }
        if let Some(d) = newest(&b, |p| ann_state(p) == want) {
            return Some(d);
        }
    }
    exact.or_else(|| newest_map(&b))
}

/// validate checks the exact-HEAD map when there is one, so the agent's loop sees its own edits.
pub fn find_map_for_validate(root: &Path, repo: &str, branch: &str, head: &str) -> Option<PathBuf> {
    let exact = branch_dir(root, repo, branch).join(head);
    if exact.join("facts.json").exists() { Some(exact) } else { find_map(root, repo, branch, head) }
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
