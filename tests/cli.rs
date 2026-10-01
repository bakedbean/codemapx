mod common;

use std::{fs, path::Path, process::{Command, Output}};

fn bin(dir: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemapx")).current_dir(dir).env("CODEMAPX_STATE_DIR", state).args(args).output().unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn collect_validate_view_round_trip() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let out = bin(&s.root, state.path(), &["collect", "--base", "main"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let dir = std::path::PathBuf::from(text(&out.stdout).trim());
    assert!(dir.starts_with(state.path().join("sample/feature__12-apply-fees")));
    assert!(dir.join("facts.json").exists());

    let out = bin(&s.root, state.path(), &["validate"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("no annotations.json"), "{}", text(&out.stderr));

    fs::copy(common::sample_dir().join("annotations.json"), dir.join("annotations.json")).unwrap();
    let out = bin(&s.root, state.path(), &["validate"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert!(text(&out.stdout).contains("0 problem(s)"));

    let out = bin(&s.root, state.path(), &["view", "--snapshot", "src/billing/apply.ts", "0"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("applyChanges"));
}

#[test]
fn collect_outside_a_repo_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = bin(dir.path(), dir.path(), &["collect"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("not a git repository"));
}

#[test]
fn view_without_a_map_points_at_the_skill() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let out = bin(&s.root, state.path(), &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("/codemapx"), "{}", text(&out.stderr));
}

#[test]
fn malformed_annotations_reported_with_position() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let dir = std::path::PathBuf::from(text(&bin(&s.root, state.path(), &["collect", "--base", "main"]).stdout).trim());
    fs::write(dir.join("annotations.json"), "{\n  \"version\": 1,\n  \"bogus\": true\n}").unwrap();
    let out = bin(&s.root, state.path(), &["validate"]);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(err.contains("annotations.json:") && err.contains("line 3"), "{err}");
}

#[test]
fn invalid_annotations_list_every_problem() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let dir = std::path::PathBuf::from(text(&bin(&s.root, state.path(), &["collect", "--base", "main"]).stdout).trim());
    let mut a: serde_json::Value = serde_json::from_str(&fs::read_to_string(common::sample_dir().join("annotations.json")).unwrap()).unwrap();
    a["dropped"] = serde_json::json!([]);
    a["trail"].as_array_mut().unwrap().pop();
    fs::write(dir.join("annotations.json"), a.to_string()).unwrap();
    let out = bin(&s.root, state.path(), &["validate"]);
    assert_eq!(out.status.code(), Some(1));
    let o = text(&out.stdout);
    assert!(o.contains("c4: candidate neither kept") && o.contains("docs/apply.md: not in trail") && o.contains("2 problem(s)"), "{o}");
}
