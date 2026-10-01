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

fn cand(id: &str, from: &str, to: &str) -> codemapx::facts::Candidate {
    codemapx::facts::Candidate {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        kind: codemapx::facts::CandidateKind::Import,
        evidence: codemapx::facts::Evidence { path: to.into(), line: 1, quote: "x".into() },
    }
}

#[test]
fn carried_link_ids_are_remapped_by_pair() {
    let root = tempfile::tempdir().unwrap();
    let mut old = facts("aaa");
    old.candidates = vec![cand("c1", "a", "b"), cand("c2", "c", "d")];
    let a = store::save_facts(root.path(), &old).unwrap();
    fs::write(a.join("annotations.json"), r#"{"links":[{"candidate":"c1","reason":"ab"}],"dropped":[{"candidate":"c2","why":"cd"}]}"#).unwrap();
    let mut new = facts("bbb");
    new.candidates = vec![cand("c1", "0", "a"), cand("c2", "a", "b")];
    let b = store::save_facts(root.path(), &new).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(b.join("annotations.json")).unwrap()).unwrap();
    assert_eq!(v["links"][0]["candidate"], "c2");
    assert_eq!(v["dropped"][0]["candidate"], "gone:c->d");
}

#[test]
fn carries_from_the_newest_map_with_annotations() {
    let root = tempfile::tempdir().unwrap();
    let a = store::save_facts(root.path(), &facts("aaa")).unwrap();
    fs::write(a.join("annotations.json"), "{\"from\":\"aaa\"}").unwrap();
    touch(&a.join("facts.json"), 100);
    let b = store::save_facts(root.path(), &facts("bbb")).unwrap();
    fs::remove_file(b.join("annotations.json")).unwrap();
    let c = store::save_facts(root.path(), &facts("ccc")).unwrap();
    assert_eq!(fs::read_to_string(c.join("annotations.json")).unwrap(), "{\"from\":\"aaa\"}");
}

#[test]
fn view_prefers_a_finished_map_validate_prefers_exact_head() {
    let root = tempfile::tempdir().unwrap();
    let a = store::save_facts(root.path(), &facts("aaa")).unwrap();
    touch(&a.join("facts.json"), 100);
    let find = |head: &str| store::find_map(root.path(), "sample", "feature/12-apply-fees", head);
    let find_exact = |head: &str| store::find_map_for_validate(root.path(), "sample", "feature/12-apply-fees", head);
    // Nothing annotated: exact head, so load can say "no annotations.json".
    let b = store::save_facts(root.path(), &facts("bbb")).unwrap();
    assert_eq!(find("bbb"), Some(b.clone()));
    // Only aaa annotated: view uses it; validate still checks bbb.
    fs::write(a.join("annotations.json"), "{\"head\":\"aaa\"}").unwrap();
    assert_eq!(find("bbb"), Some(a.clone()));
    assert_eq!(find_exact("bbb"), Some(b.clone()));
    // bbb carries aaa's annotations but the agent hasn't updated them: still aaa.
    fs::write(b.join("annotations.json"), "{\"head\":\"aaa\"}").unwrap();
    assert_eq!(find("bbb"), Some(a.clone()));
    fs::write(b.join("annotations.json"), "{\"head\":\"bbb\"}").unwrap();
    assert_eq!(find("bbb"), Some(b.clone()));
    assert_eq!(find("zzz"), Some(b));
}
