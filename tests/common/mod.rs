#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Sample {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

// Fixed identity, dates and config so the sample's commit shas are the same on every machine.
const ENV: [(&str, &str); 8] = [
    ("GIT_AUTHOR_NAME", "Sample"),
    ("GIT_AUTHOR_EMAIL", "sample@example.com"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z"),
    ("GIT_COMMITTER_NAME", "Sample"),
    ("GIT_COMMITTER_EMAIL", "sample@example.com"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
];

pub fn git_out(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(root).args(args).envs(ENV).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

pub fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample")
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// `<tmp>/sample`: `main` holds tests/sample/base; `feature/12-apply-fees` (checked out) holds tests/sample/branch.
pub fn sample() -> Sample {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sample");
    fs::create_dir(&root).unwrap();
    git_out(&root, &["init", "-q", "-b", "main"]);
    copy_tree(&sample_dir().join("base"), &root);
    git_out(&root, &["add", "-A"]);
    git_out(&root, &["commit", "-q", "-m", "base"]);
    git_out(&root, &["checkout", "-q", "-b", "feature/12-apply-fees"]);
    git_out(&root, &["rm", "-rq", "."]);
    copy_tree(&sample_dir().join("branch"), &root);
    git_out(&root, &["add", "-A"]);
    git_out(&root, &["commit", "-q", "-m", "Apply regenerated fees (#12)"]);
    Sample { _dir: dir, root }
}

/// Compares with a checked-in file; `UPDATE_GOLDEN=1` rewrites it instead (review the diff before committing).
pub fn assert_golden(rel: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| panic!("{rel} missing; run with UPDATE_GOLDEN=1 and review it"));
    assert_eq!(expected, actual, "{rel} differs; rerun with UPDATE_GOLDEN=1 and review `git diff {rel}`");
}
