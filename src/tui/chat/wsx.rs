//! Whether the worktree is a wsx workspace, so the chat agent can hand changes to its primary agent.

use std::{
    path::Path,
    process::{Command, Stdio},
};

/// `repo/slug` of the wsx workspace checked out at `root`; None without wsx or outside one.
pub fn workspace(root: &Path) -> Option<String> {
    let out = Command::new("wsx")
        .args(["workspace", "list", "--json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    workspace_at(&String::from_utf8_lossy(&out.stdout), root)
}

/// The entry of `wsx workspace list --json` whose path is `root`.
pub fn workspace_at(json: &str, root: &Path) -> Option<String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let list: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    list.iter().find_map(|w| {
        let path = Path::new(w["path"].as_str()?);
        if path.canonicalize().as_deref().unwrap_or(path) != root {
            return None;
        }
        Some(format!("{}/{}", w["repo"].as_str()?, w["slug"].as_str()?))
    })
}
