mod common;

#[test]
fn sample_repo_has_one_branch_commit_with_rename_and_delete() {
    let s = common::sample();
    let log = common::git_out(&s.root, &["log", "--format=%s", "main..HEAD"]);
    assert_eq!(log.trim(), "Apply regenerated fees (#12)");
    let names = common::git_out(&s.root, &["diff", "-M", "--name-status", "main", "HEAD"]);
    assert!(names.contains("R100\tsrc/util/format.ts\tsrc/util/fmt.ts"), "{names}");
    assert!(names.contains("D\tsrc/api/legacy.ts"), "{names}");
    assert!(!names.contains("fee-writer"), "{names}");
}

#[test]
fn sample_repo_shas_are_deterministic() {
    let (a, b) = (common::sample(), common::sample());
    let head = |s: &common::Sample| common::git_out(&s.root, &["rev-parse", "HEAD"]);
    assert_eq!(head(&a), head(&b));
}
