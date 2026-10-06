mod common;

use std::path::Path;

use codemapx::tui::chat::briefing::briefing;
use codemapx::tui::chat::agent::{AgentKind, argv, ready};
use codemapx::tui::chat::{keys::{encode_key, wrap_paste}, render::render_screen};
use ratatui::{buffer::Buffer, crossterm::event::{KeyCode, KeyEvent, KeyModifiers}, layout::Rect, style::Color};

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

fn enc(code: KeyCode, m: KeyModifiers) -> Vec<u8> {
    encode_key(KeyEvent::new(code, m))
}

#[test]
fn keys_encode_like_a_terminal() {
    let none = KeyModifiers::NONE;
    let cases: [(KeyCode, KeyModifiers, &[u8]); 16] = [
        (KeyCode::Char('a'), none, b"a"),
        (KeyCode::Char('é'), none, "é".as_bytes()),
        (KeyCode::Char('c'), KeyModifiers::CONTROL, b"\x03"),
        (KeyCode::Char('z'), KeyModifiers::CONTROL, b""),
        (KeyCode::Char('d'), KeyModifiers::CONTROL, b""),
        (KeyCode::Char('b'), KeyModifiers::ALT, b"\x1bb"),
        (KeyCode::Enter, none, b"\r"),
        (KeyCode::Backspace, none, b"\x7f"),
        (KeyCode::Tab, none, b"\t"),
        (KeyCode::BackTab, KeyModifiers::SHIFT, b"\x1b[Z"),
        (KeyCode::Esc, none, b"\x1b"),
        (KeyCode::Up, none, b"\x1b[A"),
        (KeyCode::Home, none, b"\x1b[H"),
        (KeyCode::PageDown, none, b"\x1b[6~"),
        (KeyCode::Delete, none, b"\x1b[3~"),
        (KeyCode::F(5), none, b""),
    ];
    for (code, m, want) in cases {
        assert_eq!(enc(code, m), want, "{code:?} {m:?}");
    }
    assert_eq!(wrap_paste("a\nb"), b"\x1b[200~a\nb\x1b[201~");
}

fn render(bytes: &[u8]) -> Buffer {
    let mut p = vt100::Parser::new(2, 10, 0);
    p.process(bytes);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    render_screen(p.screen(), &mut buf, Rect::new(0, 0, 10, 2));
    buf
}

#[test]
fn screens_render_text_wide_glyphs_and_color() {
    let b = render(b"hello");
    assert_eq!((0..5).map(|x| b[(x, 0)].symbol().to_string()).collect::<String>(), "hello");
    assert_eq!(b[(7, 1)].symbol(), " ");
    let b = render("世界".as_bytes());
    assert_eq!([b[(0, 0)].symbol(), b[(1, 0)].symbol(), b[(2, 0)].symbol()], ["世", " ", "界"]);
    assert_eq!(render(b"\x1b[31mX")[(0, 0)].fg, Color::Indexed(1));
}
