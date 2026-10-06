mod common;

use std::path::Path;

use codemapx::tui::chat::briefing::briefing;

#[test]
fn briefing_covers_the_map_without_diffs() {
    let map = common::sample_map();
    let b = briefing(&map, Some(Path::new("/state/map")));
    for needle in [
        "branch `feature/12-apply-fees`",
        "Read-only",
        "## Changed files, in reading order",
        "`src/billing/apply.ts` (added",
        "## Links",
        " → ",
        "git diff ",
        "/state/map/facts.json",
        "/state/map/annotations.json",
    ] {
        assert!(b.contains(needle), "missing {needle:?}\n{b}");
    }
    assert!(!b.contains("@@ "), "no diff text in the briefing");
    common::assert_golden("tests/golden/briefing.txt", &b);
}

#[test]
fn briefing_without_a_map_dir_skips_the_raw_files() {
    let b = briefing(&common::sample_map(), None);
    assert!(!b.contains("facts.json"));
}
