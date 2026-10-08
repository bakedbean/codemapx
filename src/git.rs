//! Thin wrapper over the `git` CLI, so the user's git config applies.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

use crate::facts::Status;

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: String,
    pub old_path: Option<String>,
    pub status: Status,
    pub add: u32,
    pub del: u32,
    pub binary: bool,
}

pub struct Git {
    root: PathBuf,
}

impl Git {
    pub fn open(path: &Path) -> Result<Git, String> {
        let out = run(path, &["rev-parse", "--show-toplevel"]).map_err(|_| format!("{}: not a git repository", path.display()))?;
        Ok(Git { root: PathBuf::from(out.trim()) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(&self.root, args)
    }

    pub fn head(&self) -> Result<String, String> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    pub fn branch(&self) -> Result<String, String> {
        let b = self.git(&["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
        if b == "HEAD" {
            return Err("detached HEAD; check out a branch first".into());
        }
        Ok(b)
    }

    /// Names `branch` had before `git branch -m`, newest first, read from its reflog (which a rename carries along).
    pub fn former_names(&self, branch: &str) -> Vec<String> {
        let out = self.git(&["reflog", "show", "--format=%gs", &format!("refs/heads/{branch}"), "--"]).unwrap_or_default();
        out.lines()
            .filter_map(|l| l.strip_prefix("Branch: renamed refs/heads/")?.split_once(" to refs/heads/").map(|(from, _)| from.to_string()))
            .collect()
    }

    /// Basename of the main checkout, shared by all of a repo's worktrees.
    pub fn repo_name(&self) -> Result<String, String> {
        let common = self.git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        Path::new(common.trim())
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| "can't name this repository".into())
    }

    /// Merge-base of HEAD with `base`, or with origin/main then main.
    pub fn merge_base(&self, base: Option<&str>) -> Result<String, String> {
        let refs: Vec<&str> = match base {
            Some(b) => vec![b],
            None => vec!["origin/main", "main"],
        };
        for r in &refs {
            if let Ok(out) = self.git(&["merge-base", "HEAD", r]) {
                return Ok(out.trim().to_string());
            }
        }
        Err(format!("no merge-base with {}; pass --base <ref>", refs.join(" or ")))
    }

    /// Files changed in base..HEAD (committed work only), sorted by path.
    pub fn changes(&self, base: &str) -> Result<Vec<Change>, String> {
        let counts = parse_numstat_z(&self.git(&["diff", "-M", "-z", "--numstat", base, "HEAD"])?);
        let mut out: Vec<Change> = parse_name_status_z(&self.git(&["diff", "-M", "-z", "--name-status", base, "HEAD"])?)
            .into_iter()
            .map(|(status, old_path, path)| {
                let c = counts.get(&path).copied().flatten();
                Change { binary: c.is_none(), add: c.map_or(0, |c| c.0), del: c.map_or(0, |c| c.1), path, old_path, status }
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// The file's diff from its first `@@`, or "" when there is no hunk (pure rename).
    pub fn file_diff(&self, base: &str, c: &Change) -> Result<String, String> {
        let mut args = vec!["diff", "-M", "-U3", "--no-color", "--no-ext-diff", base, "HEAD", "--"];
        if let Some(old) = &c.old_path {
            args.push(old);
        }
        args.push(&c.path);
        let out = self.git(&args)?;
        Ok(match out.find("\n@@") {
            Some(i) => out[i + 1..].trim_end_matches('\n').to_string(),
            None => String::new(),
        })
    }

    /// (sha, subject) for base..HEAD, oldest first.
    pub fn commits(&self, base: &str) -> Result<Vec<(String, String)>, String> {
        let out = self.git(&["log", "--reverse", "--format=%H%x09%s", &format!("{base}..HEAD")])?;
        Ok(out.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.into(), b.into())).collect())
    }

    /// File contents at `rev`, or None if the path doesn't exist there.
    pub fn show(&self, rev: &str, path: &str) -> Option<String> {
        self.git(&["show", &format!("{rev}:{path}")]).ok()
    }

    pub fn count_between(&self, from: &str, to: &str) -> Option<usize> {
        self.git(&["rev-list", "--count", &format!("{from}..{to}")]).ok()?.trim().parse().ok()
    }

    pub fn is_dirty(&self) -> bool {
        self.git(&["status", "--porcelain"]).is_ok_and(|s| !s.trim().is_empty())
    }
}

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `-z --numstat`: "add\tdel\tpath\0", or "add\tdel\t\0old\0new\0" for renames; "-" counts mean binary.
fn parse_numstat_z(out: &str) -> HashMap<String, Option<(u32, u32)>> {
    let mut m = HashMap::new();
    let mut t = out.split('\0');
    while let Some(head) = t.next() {
        if head.is_empty() {
            continue;
        }
        let mut f = head.splitn(3, '\t');
        let (a, d, p) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        let path = if p.is_empty() {
            t.next();
            t.next().unwrap_or("").to_string()
        } else {
            p.to_string()
        };
        m.insert(path, a.parse().ok().zip(d.parse().ok()));
    }
    m
}

/// `-z --name-status`: "M\0path\0", or "R100\0old\0new\0".
fn parse_name_status_z(out: &str) -> Vec<(Status, Option<String>, String)> {
    let mut t = out.split('\0').filter(|s| !s.is_empty());
    let mut v = vec![];
    while let Some(code) = t.next() {
        let status = match code.chars().next() {
            Some('A') => Status::Added,
            Some('D') => Status::Deleted,
            Some('R') => Status::Renamed,
            _ => Status::Modified,
        };
        if status == Status::Renamed {
            let old = t.next().unwrap_or("").to_string();
            v.push((status, Some(old), t.next().unwrap_or("").to_string()));
        } else {
            v.push((status, None, t.next().unwrap_or("").to_string()));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_z_handles_renames_binary_and_spaces() {
        let m = parse_numstat_z("3\t1\tsrc/my file é.ts\0-\t-\timg.png\0");
        assert_eq!(m["src/my file é.ts"], Some((3, 1)));
        assert_eq!(m["img.png"], None);
        let r = parse_numstat_z("0\t0\t\0old.ts\0new.ts\0");
        assert_eq!(r["new.ts"], Some((0, 0)));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn name_status_z_reads_renames() {
        let v = parse_name_status_z("M\0a.ts\0R100\0old.ts\0new.ts\0D\0gone.ts\0");
        assert_eq!(
            v,
            vec![
                (Status::Modified, None, "a.ts".into()),
                (Status::Renamed, Some("old.ts".into()), "new.ts".into()),
                (Status::Deleted, None, "gone.ts".into()),
            ]
        );
    }
}
