mod common;

use std::{fs, path::{Path, PathBuf}, process::{Command, Output}};

fn bin(dir: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemapx")).current_dir(dir).env("CODEMAPX_STATE_DIR", state).args(args).output().unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn collect_dir(root: &Path, state: &Path) -> PathBuf {
    let out = bin(root, state, &["collect", "--base", "main"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(!stdout.trim().is_empty(), "collect printed no map dir");
    let dir = PathBuf::from(stdout.trim());
    assert!(dir.starts_with(state), "{} not under {}", dir.display(), state.display());
    dir
}

#[test]
fn collect_validate_view_round_trip() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let dir = collect_dir(&s.root, state.path());
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
    let dir = collect_dir(&s.root, state.path());
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
    let dir = collect_dir(&s.root, state.path());
    let mut a: serde_json::Value = serde_json::from_str(&fs::read_to_string(common::sample_dir().join("annotations.json")).unwrap()).unwrap();
    a["dropped"] = serde_json::json!([]);
    a["trail"].as_array_mut().unwrap().pop();
    fs::write(dir.join("annotations.json"), a.to_string()).unwrap();
    let out = bin(&s.root, state.path(), &["validate"]);
    assert_eq!(out.status.code(), Some(1));
    let o = text(&out.stdout);
    assert!(o.contains("c4: candidate neither kept") && o.contains("docs/apply.md: not in trail") && o.contains("2 problem(s)"), "{o}");
}

#[test]
fn html_writes_a_page() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let dir = collect_dir(&s.root, state.path());
    fs::copy(common::sample_dir().join("annotations.json"), dir.join("annotations.json")).unwrap();
    let page = state.path().join("map.html");
    let out = bin(&s.root, state.path(), &["html", "-o", page.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(fs::read_to_string(page).unwrap().contains("<title>Apply regenerated fees</title>"));
}

#[test]
fn skill_mentions_every_annotations_field() {
    let skill = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/codemapx/SKILL.md")).unwrap();
    for field in ["\"columns\"", "\"files\"", "\"links\"", "\"dropped\"", "\"context\"", "\"context_links\"", "\"missing\"", "\"trail\"", "codemapx collect", "codemapx validate"] {
        assert!(skill.contains(field), "SKILL.md doesn't mention {field}");
    }
}

fn pairs(facts_dir: &Path, ann_dir: &Path) -> Vec<(String, String, String)> {
    let f: serde_json::Value = serde_json::from_str(&fs::read_to_string(facts_dir.join("facts.json")).unwrap()).unwrap();
    let a: serde_json::Value = serde_json::from_str(&fs::read_to_string(ann_dir.join("annotations.json")).unwrap()).unwrap();
    a["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let c = f["candidates"].as_array().unwrap().iter().find(|c| c["id"] == l["candidate"]).unwrap_or_else(|| panic!("{l}: no such candidate"));
            (c["from"].as_str().unwrap().into(), c["to"].as_str().unwrap().into(), l["reason"].as_str().unwrap().into())
        })
        .collect()
}

#[test]
fn carried_annotations_keep_reasons_on_their_pairs() {
    let s = common::sample();
    let state = tempfile::tempdir().unwrap();
    let old = collect_dir(&s.root, state.path());
    fs::copy(common::sample_dir().join("annotations.json"), old.join("annotations.json")).unwrap();
    // A candidate (apply.ts -> aaa.ts) that sorts before every existing one shifts all positional ids.
    fs::write(s.root.join("src/api/aaa.ts"), "import { applyChanges } from '../billing/apply';\n\nexport const run = () => applyChanges([]);\n").unwrap();
    common::git_out(&s.root, &["add", "-A"]);
    common::git_out(&s.root, &["commit", "-q", "-m", "aaa"]);
    let new = collect_dir(&s.root, state.path());
    assert_ne!(old, new);
    assert_eq!(pairs(&new, &new), pairs(&old, &old));
}
