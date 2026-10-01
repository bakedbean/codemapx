mod common;

use codemapx::{
    annotations::{Annotations, ContextCard, Dropped, KeptLink, Missing},
    map::{CardKind, merge},
};

fn problems(edit: impl FnOnce(&mut Annotations)) -> Vec<String> {
    let mut a = common::sample_annotations();
    edit(&mut a);
    merge(&common::sample_facts(), &a, &common::read_branch).err().unwrap_or_default()
}

fn assert_has(ps: &[String], needle: &str) {
    assert!(ps.iter().any(|p| p.contains(needle)), "no problem containing {needle:?} in {ps:#?}");
}

#[test]
fn sample_merges_cleanly() {
    let m = common::sample_map();
    assert_eq!(m.cards.len(), 10);
    assert_eq!(m.columns.len(), 5);
    assert_eq!(m.links.len(), 5);
    assert_eq!(m.trail.len(), 9);
    assert!(!m.annotations_stale);
    let enqueue = &m.cards[m.card_index("enqueue").unwrap()];
    assert_eq!((enqueue.kind, enqueue.column), (CardKind::Missing, 2));
    let apply = &m.cards[m.card_index("src/billing/apply.ts").unwrap()];
    assert_eq!((apply.name.as_str(), apply.dir.as_str()), ("apply.ts", "src/billing/"));
    assert_eq!(apply.outline[0].note, "Formats each amount with millsToDecimal.");
    assert_eq!(m.cards[m.card_index("src/jobs/fee-writer.ts").unwrap()].kind, CardKind::Context);
}

#[test]
fn rule1_every_changed_file_in_one_column_and_trail() {
    assert_has(&problems(|a| a.columns[2].files.retain(|f| f != "src/util/fmt.ts")), "src/util/fmt.ts: not in any column");
    assert_has(&problems(|a| a.columns[0].files.push("src/billing/apply.ts".into())), "src/billing/apply.ts: in more than one column");
    assert_has(&problems(|a| a.columns[0].files.push("nope.ts".into())), "nope.ts: in column");
    assert_has(&problems(|a| a.trail.retain(|f| f != "docs/apply.md")), "docs/apply.md: not in trail");
    assert_has(&problems(|a| a.trail.push("docs/apply.md".into())), "docs/apply.md: in trail more than once");
    assert_has(&problems(|a| a.trail.push("enqueue".into())), "enqueue: in trail but not a changed or context file");
}

#[test]
fn rule2_every_candidate_judged_once() {
    assert_has(&problems(|a| a.dropped.clear()), "c4: candidate neither kept in links nor dropped");
    assert_has(&problems(|a| a.links.push(KeptLink { candidate: "c9".into(), reason: "x".into() })), "c9: unknown candidate");
    assert_has(&problems(|a| a.dropped.push(Dropped { candidate: "c1".into(), why: "x".into() })), "c1: candidate listed more than once");
}

#[test]
fn rule3_and_4_file_notes() {
    assert_has(&problems(|a| { a.files.remove("docs/apply.md"); }), "docs/apply.md: missing \"what\"");
    assert_has(&problems(|a| { a.files.insert("nope.ts".into(), a.files["docs/apply.md"].clone()); }), "nope.ts: not a changed file");
    assert_has(
        &problems(|a| { a.files.get_mut("src/billing/apply.ts").unwrap().outline.insert("ghost".into(), "x".into()); }),
        "src/billing/apply.ts: outline note for unknown entry \"ghost\"",
    );
}

#[test]
fn rule5_context_paths() {
    assert_has(&problems(|a| a.context.push(ContextCard { path: "src/billing/apply.ts".into(), what: "x".into() })), "src/billing/apply.ts: context path is a changed file");
    assert_has(&problems(|a| a.context[0].path = "src/jobs/nope.ts".into()), "src/jobs/nope.ts: context path does not exist at HEAD");
}

#[test]
fn rule6_context_links_cite_real_lines() {
    assert_has(&problems(|a| a.context_links[0].evidence.quote = "made up".into()), "src/jobs/fee-writer.ts:3: evidence quote not found on that line");
    assert_has(&problems(|a| a.context_links[0].evidence.line = 99), "src/jobs/fee-writer.ts:99: line out of range");
    assert_has(&problems(|a| a.context_links[0].to = "nope.ts".into()), "nope.ts: link endpoint is not a changed or context file");
}

#[test]
fn rule7_missing_cards() {
    assert_has(&problems(|a| a.missing[0].near = "nope.ts".into()), "enqueue: near \"nope.ts\" is not a changed file");
    let dup = Missing { id: "enqueue".into(), name: "x".into(), why: "x".into(), near: "src/api/route.ts".into() };
    assert_has(&problems(|a| a.missing.push(dup)), "enqueue: duplicate missing id");
}

#[test]
fn rule8_head_mismatch_is_stale_not_invalid() {
    let mut a = common::sample_annotations();
    a.head = "0000000".into();
    let m = merge(&common::sample_facts(), &a, &common::read_branch).unwrap();
    assert!(m.annotations_stale);
}

#[test]
fn quote_matches_crlf_files() {
    let mut a = common::sample_annotations();
    a.context_links[0].evidence.quote = "export function writeFee(fee: Fee): void {".into();
    let crlf = |p: &str| common::read_branch(p).map(|s| s.replace('\n', "\r\n"));
    assert!(merge(&common::sample_facts(), &a, &crlf).is_ok());
}

#[test]
fn unknown_fields_are_rejected_with_position() {
    let err = codemapx::annotations::parse("{\n  \"version\": 1,\n  \"titel\": \"x\"\n}").unwrap_err();
    assert!(err.starts_with("annotations.json: ") && err.contains("titel") && err.contains("line 3"), "{err}");
}
