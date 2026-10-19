//! Whether the worktree is a wsx workspace, so the chat agent can hand changes to its primary agent.

use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// How long agent start waits on wsx before going without it.
const TIMEOUT: Duration = Duration::from_secs(2);

/// The command the chat may run unprompted: a send to this workspace's primary agent and no other.
pub fn send_prefix(ws: &str) -> String {
    format!("wsx agent send --workspace {ws} primary")
}

/// `repo/slug` of the wsx workspace checked out at `root`; None without wsx, outside one, or if wsx takes over `TIMEOUT`.
pub fn workspace(root: &Path) -> Option<String> {
    let json = run(Command::new("wsx").args(["workspace", "list", "--json"]), TIMEOUT)?;
    workspace_at(&json, root)
}

/// `cmd`'s stdout if it exits 0 within `timeout`; a slower child is killed.
pub fn run(cmd: &mut Command, timeout: Duration) -> Option<String> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut out = child.stdout.take()?;
    // Read on a thread so a large listing can't fill the pipe and stall the child.
    let reader = thread::spawn(move || {
        let mut s = String::new();
        out.read_to_string(&mut s).ok().map(|_| s)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let s = reader.join().ok()??;
    status.success().then_some(s)
}

/// The entry of `wsx workspace list --json` whose path is `root`.
pub fn workspace_at(json: &str, root: &Path) -> Option<String> {
    let list: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    let find = |at: &dyn Fn(&Path) -> bool| {
        list.iter().find_map(|w| {
            if !at(Path::new(w["path"].as_str()?)) {
                return None;
            }
            Some(format!("{}/{}", w["repo"].as_str()?, w["slug"].as_str()?))
        })
    };
    // Paths as written first; resolving every workspace's path (symlinks, /private) only when that misses.
    find(&|p| p == root).or_else(|| {
        let canon = root.canonicalize().ok()?;
        find(&|p| p.canonicalize().ok().as_ref() == Some(&canon))
    })
}
