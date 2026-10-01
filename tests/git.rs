mod common;

use codemapx::{facts::Status, git::Git};

#[test]
fn changes_against_merge_base() {
    let s = common::sample();
    let g = Git::open(&s.root).unwrap();
    let base = g.merge_base(Some("main")).unwrap();
    let ch = g.changes(&base).unwrap();
    let got: Vec<(&str, Status)> = ch.iter().map(|c| (c.path.as_str(), c.status)).collect();
    use Status::*;
    assert_eq!(
        got,
        vec![
            ("docs/apply.md", Added),
            ("src/api/legacy.ts", Deleted),
            ("src/api/route.ts", Added),
            ("src/billing/apply.ts", Added),
            ("src/billing/mills.test.ts", Modified),
            ("src/billing/mills.ts", Modified),
            ("src/billing/types.ts", Modified),
            ("src/util/fmt.ts", Renamed),
        ]
    );
    let get = |p: &str| ch.iter().find(|c| c.path == p).unwrap();
    assert_eq!(get("src/util/fmt.ts").old_path.as_deref(), Some("src/util/format.ts"));
    assert_eq!((get("src/billing/types.ts").add, get("src/billing/types.ts").del), (5, 0));
}

#[test]
fn file_diff_starts_at_first_hunk_and_rename_is_empty() {
    let s = common::sample();
    let g = Git::open(&s.root).unwrap();
    let base = g.merge_base(Some("main")).unwrap();
    let ch = g.changes(&base).unwrap();
    let types = ch.iter().find(|c| c.path == "src/billing/types.ts").unwrap();
    let d = g.file_diff(&base, types).unwrap();
    assert!(d.starts_with("@@ -"), "{d}");
    assert!(d.contains("+export interface Insertion {"));
    let fmt = ch.iter().find(|c| c.path == "src/util/fmt.ts").unwrap();
    assert_eq!(g.file_diff(&base, fmt).unwrap(), "");
}

#[test]
fn names_commits_and_reads_files_at_head() {
    let s = common::sample();
    let g = Git::open(&s.root).unwrap();
    let base = g.merge_base(None).unwrap();
    assert_eq!(g.branch().unwrap(), "feature/12-apply-fees");
    assert_eq!(g.repo_name().unwrap(), "sample");
    let commits = g.commits(&base).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].1, "Apply regenerated fees (#12)");
    let head = g.head().unwrap();
    assert!(g.show(&head, "src/api/route.ts").unwrap().contains("applyChanges"));
    assert!(g.show(&head, "src/api/legacy.ts").is_none());
    assert_eq!(g.count_between(&base, &head), Some(1));
    assert!(!g.is_dirty());
}

#[test]
fn detached_head_is_an_error() {
    let s = common::sample();
    common::git_out(&s.root, &["checkout", "-q", "--detach"]);
    let err = Git::open(&s.root).unwrap().branch().unwrap_err();
    assert!(err.contains("detached HEAD"), "{err}");
}

#[test]
fn open_outside_a_repo_fails() {
    let dir = tempfile::tempdir().unwrap();
    let err = Git::open(dir.path()).err().unwrap();
    assert!(err.contains("not a git repository"), "{err}");
}
