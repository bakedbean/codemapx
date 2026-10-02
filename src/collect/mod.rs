//! `codemapx collect`: facts about a branch, from git and tree-sitter.

pub mod candidates;
pub mod imports;
pub mod outline;

use std::collections::BTreeSet;

use crate::{
    diff,
    facts::{Commit, Facts, FileFacts, Status},
    git::Git,
};
use candidates::{FileInfo, candidates};
use imports::Resolver;

pub fn collect(git: &Git, base: Option<&str>) -> Result<Facts, String> {
    let branch = git.branch()?;
    let base = git.merge_base(base)?;
    let head = git.head()?;
    let changes = git.changes(&base)?;
    if changes.is_empty() {
        return Err("no changes vs base".into());
    }
    let read = |p: &str| git.show(&head, p);
    let mut warnings = vec![];
    if git.is_dirty() {
        warnings.push("worktree has uncommitted changes; the map covers committed work only".to_string());
    }
    let (mut files, mut infos) = (vec![], vec![]);
    for c in &changes {
        let diff = if c.binary { String::new() } else { git.file_diff(&base, c)? };
        let added: Vec<(usize, String)> = diff::added_lines(&diff).into_iter().map(|(n, t)| (n, t.to_string())).collect();
        let added_nums: Vec<usize> = added.iter().map(|(n, _)| *n).collect();
        let src = if c.status == Status::Deleted || c.binary { None } else { read(&c.path) };
        let (mut items, mut fns, mut imps, mut exports) = (vec![], vec![], vec![], BTreeSet::new());
        if let (Some(src), Some(lang)) = (&src, outline::lang_for(&c.path))
            && let Some(tree) = outline::parse(lang, src)
        {
            imps = imports::imports(&tree, src);
            if tree.root_node().has_error() {
                warnings.push(format!("{}: parse errors, outline may be partial", c.path));
            }
            let decls = outline::declarations(&tree, src);
            items = outline::outline(&decls, &added_nums);
            fns = outline::functions(&decls, &added_nums, &diff::deleted_at(&diff));
            exports = outline::changed_exports(&decls, &added_nums);
        }
        infos.push(FileInfo {
            path: c.path.clone(),
            exists: src.is_some(),
            added,
            imports: imps,
            changed_exports: exports,
            first_line: src.as_deref().and_then(|s| s.lines().next()).unwrap_or("").trim().to_string(),
        });
        files.push(FileFacts {
            path: c.path.clone(),
            status: c.status,
            old_path: c.old_path.clone(),
            binary: c.binary,
            add: c.add,
            del: c.del,
            diff,
            outline: items,
            functions: fns,
        });
    }
    let existing: BTreeSet<&str> = infos.iter().filter(|f| f.exists).map(|f| f.path.as_str()).collect();
    let resolver = Resolver::new(&read);
    let candidates = candidates(&infos, &|from, spec| resolver.resolve(from, spec, &|p| existing.contains(p)));
    let commits = git
        .commits(&base)?
        .into_iter()
        .map(|(sha, subject)| Commit { issues: issue_refs(&subject), sha, subject })
        .collect();
    Ok(Facts {
        version: 1,
        repo: git.repo_name()?,
        branch_issue: branch_issue(&branch),
        branch,
        base,
        head,
        commits,
        files,
        candidates,
        warnings,
    })
}

pub fn issue_refs(s: &str) -> Vec<String> {
    s.match_indices('#')
        .filter_map(|(i, _)| {
            let digits: String = s[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty()).then(|| format!("#{digits}"))
        })
        .collect()
}

/// Leading number of the branch's last segment: `eben/3046-x` → `#3046`.
pub fn branch_issue(branch: &str) -> Option<String> {
    let last = branch.rsplit('/').next()?;
    let digits: String = last.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then(|| format!("#{digits}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_issue_refs() {
        assert_eq!(issue_refs("Fix #12 and #3046, not #x"), vec!["#12", "#3046"]);
        assert_eq!(branch_issue("eben/3046-invoice-v2"), Some("#3046".into()));
        assert_eq!(branch_issue("feature/apply"), None);
    }
}
