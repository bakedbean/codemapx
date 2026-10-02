mod common;

use codemapx::{
    collect::collect,
    facts::{self, CandidateKind::*},
    git::Git,
};

#[test]
fn collects_sample_facts() {
    let s = common::sample();
    let f = collect(&Git::open(&s.root).unwrap(), Some("main")).unwrap();
    assert_eq!((f.repo.as_str(), f.branch.as_str()), ("sample", "feature/12-apply-fees"));
    assert_eq!(f.branch_issue.as_deref(), Some("#12"));
    assert_eq!(f.commits.len(), 1);
    assert_eq!(f.commits[0].issues, vec!["#12"]);
    let c: Vec<_> = f.candidates.iter().map(|c| (c.id.as_str(), c.from.as_str(), c.to.as_str(), c.kind)).collect();
    assert_eq!(
        c,
        vec![
            ("c1", "src/billing/apply.ts", "src/api/route.ts", ExportUse),
            ("c2", "src/billing/mills.ts", "src/billing/apply.ts", ExportUse),
            ("c3", "src/billing/mills.ts", "src/billing/mills.test.ts", ExportUse),
            ("c4", "src/billing/types.ts", "src/api/route.ts", Import),
            ("c5", "src/billing/types.ts", "src/billing/apply.ts", ExportUse),
        ]
    );
    let outline = |p: &str| {
        f.files.iter().find(|x| x.path == p).unwrap().outline.iter().map(|o| (o.name.clone(), o.start, o.end)).collect::<Vec<_>>()
    };
    assert_eq!(outline("src/billing/types.ts"), vec![("Insertion".to_string(), 6, 9)]);
    assert_eq!(outline("src/billing/mills.ts"), vec![("millsToDecimal".to_string(), 5, 8)]);
    assert_eq!(outline("src/billing/apply.ts"), vec![("applyChanges".to_string(), 4, 6)]);
    assert_eq!(outline("src/api/route.ts"), vec![("handle".to_string(), 4, 6)]);
    let mills = &f.files.iter().find(|x| x.path == "src/billing/mills.ts").unwrap().functions;
    let got: Vec<(&str, usize, bool)> = mills.iter().map(|x| (x.name.as_str(), x.start, x.changed)).collect();
    assert_eq!(got, vec![("toMills", 1, false), ("millsToDecimal", 5, true)]);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    common::assert_golden("tests/sample/expected-facts.json", &facts::to_json(&f));
}

#[test]
fn collect_is_deterministic() {
    let s = common::sample();
    let g = Git::open(&s.root).unwrap();
    assert_eq!(facts::to_json(&collect(&g, Some("main")).unwrap()), facts::to_json(&collect(&g, Some("main")).unwrap()));
}

#[test]
fn no_changes_is_an_error() {
    let s = common::sample();
    common::git_out(&s.root, &["checkout", "-q", "-b", "empty", "main"]);
    let err = collect(&Git::open(&s.root).unwrap(), Some("main")).unwrap_err();
    assert_eq!(err, "no changes vs base");
}

#[test]
fn dirty_worktree_warns() {
    let s = common::sample();
    std::fs::write(s.root.join("src/billing/apply.ts"), "// uncommitted\n").unwrap();
    let f = collect(&Git::open(&s.root).unwrap(), Some("main")).unwrap();
    assert!(f.warnings.iter().any(|w| w.contains("uncommitted")), "{:?}", f.warnings);
    assert!(f.files.iter().find(|x| x.path == "src/billing/apply.ts").unwrap().diff.contains("applyChanges"));
}
