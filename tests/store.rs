use std::{fs, time::{Duration, SystemTime}};

use codemapx::{facts::Facts, store};

fn facts(head: &str) -> Facts {
    Facts {
        version: 1,
        repo: "sample".into(),
        branch: "feature/12-apply-fees".into(),
        branch_issue: None,
        base: "b".into(),
        head: head.into(),
        commits: vec![],
        files: vec![],
        candidates: vec![],
        warnings: vec![],
    }
}

fn touch(path: &std::path::Path, secs: u64) {
    fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
}

#[test]
fn saves_under_repo_branch_head_with_slashes_escaped() {
    let root = tempfile::tempdir().unwrap();
    let dir = store::save_facts(root.path(), &facts("aaa")).unwrap();
    assert_eq!(dir, root.path().join("sample/feature__12-apply-fees/aaa"));
    assert!(dir.join("facts.json").exists());
}

#[test]
fn carries_annotations_forward_to_a_new_head() {
    let root = tempfile::tempdir().unwrap();
    let a = store::save_facts(root.path(), &facts("aaa")).unwrap();
    fs::write(a.join("annotations.json"), "{\"from\":\"aaa\"}").unwrap();
    let b = store::save_facts(root.path(), &facts("bbb")).unwrap();
    assert_eq!(fs::read_to_string(b.join("annotations.json")).unwrap(), "{\"from\":\"aaa\"}");
}

#[test]
fn finds_exact_head_else_newest() {
    let root = tempfile::tempdir().unwrap();
    let a = store::save_facts(root.path(), &facts("aaa")).unwrap();
    let b = store::save_facts(root.path(), &facts("bbb")).unwrap();
    touch(&a.join("facts.json"), 100);
    touch(&b.join("facts.json"), 200);
    let find = |head: &str| store::find_map(root.path(), "sample", "feature/12-apply-fees", head);
    assert_eq!(find("aaa"), Some(a));
    assert_eq!(find("zzz"), Some(b));
    assert_eq!(store::find_map(root.path(), "sample", "other", "aaa"), None);
}
