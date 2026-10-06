mod common;

use std::path::Path;

use codemapx::tui::chat::briefing::briefing;
use codemapx::tui::chat::agent::{AgentKind, argv, ready};

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

#[test]
fn agent_kind_comes_from_the_env() {
    assert_eq!(AgentKind::from_env(None), Ok(AgentKind::Claude));
    assert_eq!(AgentKind::from_env(Some(" ")), Ok(AgentKind::Claude));
    assert_eq!(AgentKind::from_env(Some("codex")), Ok(AgentKind::Codex));
    assert_eq!(AgentKind::from_env(Some("gpt")), Err("CODEMAPX_AGENT=gpt: expected claude or codex".into()));
}

#[test]
fn claude_argv_is_read_only_with_the_briefing() {
    assert_eq!(
        argv(AgentKind::Claude, None, "brief"),
        ["claude", "--allowedTools", "Read", "Grep", "Glob", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)", "--append-system-prompt", "brief"]
    );
    assert_eq!(argv(AgentKind::Claude, Some("/opt/claude"), "b")[0], "/opt/claude");
    assert_eq!(argv(AgentKind::Claude, Some(""), "b")[0], "claude");
}

#[test]
fn codex_argv_escapes_the_briefing() {
    let v = argv(AgentKind::Codex, None, "say \"hi\"\nC:\\x\t.");
    assert_eq!(v[..6], ["codex", "-s", "read-only", "-a", "on-request", "-c"]);
    assert_eq!(v[6], r#"developer_instructions="say \"hi\"\nC:\\x\t.""#);
    assert_eq!(v.len(), 7);
}

#[test]
fn claude_is_ready_on_the_alternate_screen() {
    let mut p = vt100::Parser::new(5, 40, 0);
    assert!(!ready(AgentKind::Claude, p.screen()));
    p.process(b"\x1b[?1049h");
    assert!(ready(AgentKind::Claude, p.screen()));
}

#[test]
fn codex_is_ready_at_its_composer_not_its_trust_dialog() {
    let mut p = vt100::Parser::new(5, 40, 0);
    assert!(!ready(AgentKind::Codex, p.screen()));
    p.process("\x1b[?25l› 1. Yes, continue".as_bytes());
    assert!(!ready(AgentKind::Codex, p.screen()), "cursor hidden: trust dialog");
    let mut p = vt100::Parser::new(5, 40, 0);
    p.process("\r\n  › Ask Codex to do anything".as_bytes());
    assert!(ready(AgentKind::Codex, p.screen()));
}
