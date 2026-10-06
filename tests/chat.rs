mod common;

use std::{path::{Path, PathBuf}, sync::mpsc, time::{Duration, Instant}};

use codemapx::tui::{self, App, Pane, keys::{self, Action}};
use codemapx::tui::chat::briefing::briefing;
use codemapx::tui::chat::{Chat, reference};
use codemapx::tui::chat::pty::{Session, settled};
use codemapx::tui::chat::agent::{AgentKind, argv, ready};
use codemapx::tui::chat::{keys::{encode_key, wrap_paste}, render::render_screen};
use ratatui::{buffer::Buffer, crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind}, layout::Rect, style::Color};

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
        ["claude", "--permission-mode", "default", "--allowedTools", "Read", "Grep", "Glob", "--append-system-prompt", "brief"]
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
fn claude_is_ready_at_its_composer_on_the_alternate_screen() {
    let ready_after = |bytes: &str| {
        let mut p = vt100::Parser::new(8, 40, 0);
        p.process(bytes.as_bytes());
        ready(AgentKind::Claude, p.screen())
    };
    assert!(!ready_after("\x1b[?1049h"), "no composer");
    assert!(!ready_after("\x1b[?1049hDo you trust the files in this folder?\r\n❯ 1. Yes, proceed\r\n  2. No, exit"), "trust dialog");
    assert!(ready_after("\x1b[?1049h────────\r\n❯ \r\n────────"), "composer");
    assert!(!ready_after("────────\r\n❯ \r\n────────"), "composer without the alternate screen");
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

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

/// Polls `done` until it holds or `secs` pass, sleeping on the wake channel in between.
fn wait_for(rx: &mpsc::Receiver<()>, secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if done() {
            return true;
        }
        let _ = rx.recv_timeout(Duration::from_millis(50));
    }
    done()
}

#[test]
fn a_session_shows_output_and_resizes() {
    let (tx, rx) = mpsc::sync_channel(1);
    let mut s = Session::spawn(&sh("printf hi; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains("hi")));
    s.resize(10, 40);
    assert_eq!(s.parser().screen().size(), (10, 40));
    assert_eq!(s.exit_code(), None);
}

#[test]
fn exit_is_observed() {
    let (tx, rx) = mpsc::sync_channel(1);
    let s = Session::spawn(&sh("exit 3"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.exit_code().is_some()));
    assert_eq!(s.exit_code(), Some(3));
}

#[test]
fn missing_binary_is_an_error() {
    let (tx, _rx) = mpsc::sync_channel(1);
    let e = Session::spawn(&["/nope/agent".to_string()], Path::new("/"), 5, 20, tx).err().unwrap();
    assert!(e.starts_with("can't run /nope/agent: "), "{e}");
}

#[test]
fn settled_needs_age_quiet_and_some_output() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    assert!(!settled(t0, None, ms(5000)), "no output yet");
    assert!(!settled(t0, Some(ms(100)), ms(1000)), "too young");
    assert!(!settled(t0, Some(ms(1400)), ms(1600)), "not quiet");
    assert!(settled(t0, Some(ms(1100)), ms(1600)));
}

#[test]
fn wheel_reports_only_when_the_program_asks_for_mouse() {
    let (tx, rx) = mpsc::sync_channel(1);
    let s = Session::spawn(&sh("printf '\\033[?1000h\\033[?1006hm'; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains('m')));
    assert_eq!(s.wheel_bytes(true, 3, 2), Some(b"\x1b[<64;3;2M".to_vec()));
    let (tx, rx) = mpsc::sync_channel(1);
    let s = Session::spawn(&sh("printf x; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains('x')));
    assert_eq!(s.wheel_bytes(true, 3, 2), None);
}

fn app() -> App {
    App::new(common::sample_map(), PathBuf::from("/wt"))
}

fn press(a: &mut App, code: KeyCode, m: KeyModifiers) -> Action {
    keys::handle(a, KeyEvent::new(code, m))
}

#[test]
fn a_shows_the_panel_focused_and_asks_for_an_agent() {
    let mut a = app();
    assert!(matches!(press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE), Action::StartChat));
    assert_eq!(a.focus, Pane::Chat);
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("chat · claude · ctrl-x leaves"), "{frame}");
    assert!(frame.contains("starting the agent"), "{frame}");
    assert_eq!(a.chat_pane.width, 57, "40% of 180 less the 20-column minimap and the functions panel's 16-column minimum");
    assert_eq!(a.chat_pane.right(), a.minimap.x, "sits between the diff and the minimap");
}

#[test]
fn ctrl_x_leaves_and_a_hides() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    tui::snapshot(&mut a, 180, 50);
    press(&mut a, KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(a.focus, Pane::Diff);
    assert!(matches!(press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE), Action::None));
    tui::snapshot(&mut a, 180, 50);
    assert_eq!(a.chat_pane.width, 0);
    // Hiding while focused hands focus to the diff.
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    a.focus = Pane::Chat;
    a.toggle_chat();
    assert_eq!(a.focus, Pane::Diff);
}

#[test]
fn focused_chat_without_an_agent_restarts_on_enter_and_keeps_other_keys() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(matches!(press(&mut a, KeyCode::Enter, KeyModifiers::NONE), Action::StartChat));
    assert!(matches!(press(&mut a, KeyCode::Char('q'), KeyModifiers::NONE), Action::Quit));
}

#[test]
fn focused_chat_forwards_quit_keys() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    tui::snapshot(&mut a, 180, 50);
    let chat = a.chat.as_mut().unwrap();
    chat.start(&sh("cat >/dev/null"), Path::new("/"), 5, 20);
    assert!(chat.live());
    for (code, m) in [(KeyCode::Char('q'), KeyModifiers::NONE), (KeyCode::Char('c'), KeyModifiers::CONTROL), (KeyCode::Esc, KeyModifiers::NONE), (KeyCode::Tab, KeyModifiers::NONE)] {
        assert!(matches!(press(&mut a, code, m), Action::None), "{code:?}");
        assert_eq!(a.focus, Pane::Chat, "{code:?}");
    }
}

#[test]
fn tab_reaches_the_chat_only_while_shown() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Map, "hidden: diff wraps to the map");
    a.show_chat = true;
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Chat);
}

#[test]
fn chat_width_leaves_the_diff_forty_columns() {
    let mut a = app();
    a.show_chat = true;
    for mm in [None, Some(200)] {
        a.minimap_width = mm;
        for w in [100u16, 120, 130, 150, 180] {
            tui::snapshot(&mut a, w, 50);
            let diff = a.panes[2].width - a.fns_pane.width - a.chat_pane.width - a.minimap.width;
            assert!(a.chat_pane.width >= 40 && diff >= 40, "w={w} minimap {mm:?}: chat {} diff {diff}", a.chat_pane.width);
        }
    }
}

#[test]
fn chat_errors_show_in_the_panel() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    a.chat.as_mut().unwrap().start(&["/nope/agent".to_string()], Path::new("/"), 5, 20);
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("can't run /nope/agent"), "{frame}");
}

fn mouse_at(a: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    tui::mouse::handle(a, MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
}

fn chat_app() -> App {
    let mut a = app();
    a.show_chat = true;
    tui::snapshot(&mut a, 180, 50);
    a
}

#[test]
fn clicking_the_chat_focuses_it() {
    let mut a = chat_app();
    let c = a.chat_pane;
    mouse_at(&mut a, MouseEventKind::Down(MouseButton::Left), c.x + 5, c.y + 3);
    mouse_at(&mut a, MouseEventKind::Up(MouseButton::Left), c.x + 5, c.y + 3);
    assert_eq!(a.focus, Pane::Chat);
}

#[test]
fn dragging_the_chat_border_resizes_it_within_limits() {
    let mut a = chat_app();
    let (c, diff) = (a.chat_pane, a.panes[2]);
    let row = c.y + 2;
    let drag = |a: &mut App, from: u16, to: u16| {
        mouse_at(a, MouseEventKind::Down(MouseButton::Left), from, row);
        mouse_at(a, MouseEventKind::Drag(MouseButton::Left), to, row);
        mouse_at(a, MouseEventKind::Up(MouseButton::Left), to, row);
        tui::snapshot(a, 180, 50);
    };
    drag(&mut a, c.x, c.x - 10);
    assert_eq!(a.chat_pane.width, c.width + 10);
    assert_eq!(a.panes[2], diff);
    assert_eq!(a.focus, Pane::Map, "a border drag doesn't focus the chat");
    let x = a.chat_pane.x;
    drag(&mut a, x, 179);
    assert_eq!(a.chat_pane.width, 40);
    let x = a.chat_pane.x;
    drag(&mut a, x, 0);
    assert_eq!(a.chat_pane.width, 104, "180 less the minimap (20), the functions minimum (16) and the diff (40)");
    assert_eq!(a.fns_pane.width, 16);
    assert_eq!(a.panes[2].width - a.fns_pane.width - a.chat_pane.width - a.minimap.width, 40);
}

#[test]
fn the_wheel_over_an_empty_chat_leaves_the_diff_alone() {
    let mut a = chat_app();
    let c = a.chat_pane;
    mouse_at(&mut a, MouseEventKind::ScrollDown, c.x + 5, c.y + 3);
    assert_eq!(a.scroll, 0);
}

fn at(a: &mut App, id: &str) {
    let i = a.map.card_index(id).unwrap();
    a.select(i);
}

#[test]
fn references_follow_the_pane_focus_came_from() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 50);
    let f = &a.card().functions[0];
    assert_eq!(reference(&a, Pane::Functions), Some(format!("src/billing/mills.ts:{}-{} ", f.start, f.end)));
    at(&mut a, "src/billing/apply.ts");
    let o = &a.card().outline[0];
    assert_eq!(reference(&a, Pane::Inside), Some(format!("src/billing/apply.ts:{}-{} ", o.start, o.end)));
    a.jump_to_line(4);
    assert_eq!(reference(&a, Pane::Diff), Some("src/billing/apply.ts:4 ".into()));
    a.hl = None;
    a.scroll = 0;
    let r = reference(&a, Pane::Map).unwrap();
    assert!(r.starts_with("src/billing/apply.ts:1-"), "{r}");
}

#[test]
fn no_reference_for_missing_or_deleted_files() {
    let mut a = app();
    at(&mut a, "enqueue");
    assert_eq!(reference(&a, Pane::Diff), None);
    at(&mut a, "src/api/legacy.ts");
    assert_eq!(reference(&a, Pane::Diff), None);
}

#[test]
fn focusing_the_chat_queues_a_new_reference_once() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    tui::snapshot(&mut a, 180, 50);
    let queued = a.chat.as_ref().unwrap().queued.clone();
    assert!(queued.as_deref().is_some_and(|r| r.starts_with("src/billing/apply.ts:1-")), "{queued:?}");
    // Already typed: refocusing on the same lines queues nothing.
    let chat = a.chat.as_mut().unwrap();
    chat.last_ref = queued;
    chat.queued = None;
    press(&mut a, KeyCode::Char('x'), KeyModifiers::CONTROL);
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Chat);
    assert!(a.chat.as_ref().unwrap().queued.is_none());
}

/// An alternate screen with claude's composer between its rules.
const COMPOSER: &str = "\\033[?1049h────────\\r\\n❯ \\r\\n────────";

#[test]
fn a_queued_reference_is_typed_once_the_composer_is_up() {
    let mut chat = codemapx::tui::chat::Chat::new(AgentKind::Claude);
    chat.start(&sh(&format!("printf '{COMPOSER}'; exec cat")), Path::new("/"), 5, 40);
    chat.queue("src/a.ts:1-2 ".into());
    let end = Instant::now() + Duration::from_secs(5);
    while chat.queued.is_some() && Instant::now() < end {
        chat.tick(Instant::now(), true);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(chat.queued.is_none());
    assert_eq!(chat.last_ref.as_deref(), Some("src/a.ts:1-2 "));
    let s = chat.session.as_ref().unwrap();
    let mut seen = false;
    for _ in 0..40 {
        seen = s.parser().screen().contents().contains("src/a.ts:1-2");
        if seen {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(seen, "echoed by the tty");
}

#[test]
fn a_queued_reference_is_dropped_when_the_chat_loses_focus() {
    let mut chat = Chat::new(AgentKind::Claude);
    chat.last_ref = Some("old ".into());
    chat.queue("src/a.ts ".into());
    chat.tick(Instant::now(), false);
    assert!(chat.queued.is_none());
    assert_eq!(chat.last_ref.as_deref(), Some("old "));
}

#[test]
fn typing_into_a_ready_composer_drops_a_queued_reference() {
    let mut chat = Chat::new(AgentKind::Claude);
    chat.start(&sh(&format!("printf '{COMPOSER}'; exec cat")), Path::new("/"), 5, 40);
    let s = || chat.session.as_ref().unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !ready(AgentKind::Claude, s().parser().screen()) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    chat.queue("src/a.ts ".into());
    // A swallowed key (ctrl-d, ctrl-z, F-keys) encodes to nothing and isn't typing.
    chat.write(&encode_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)));
    assert!(chat.queued.is_some());
    chat.write(b"x");
    assert!(chat.queued.is_none());
}

#[test]
fn queueing_the_last_typed_ref_drops_a_pending_one() {
    let mut chat = Chat::new(AgentKind::Claude);
    chat.last_ref = Some("x".into());
    chat.queue("y".into());
    chat.queue("x".into());
    assert!(chat.queued.is_none());
}

#[test]
fn wakes_coalesce_while_nobody_drains_them() {
    let (tx, rx) = mpsc::sync_channel(1);
    let s = Session::spawn(&sh("i=0; while [ $i -lt 200 ]; do printf 'line %s\\n' $i; i=$((i+1)); done; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(s.parser().screen().contents().contains("line 199"), "the reader never blocks on a full channel");
    assert!(rx.try_iter().count() <= 1);
}
