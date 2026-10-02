mod common;

use std::path::PathBuf;

use codemapx::tui::{ColumnView::*, column_layout};
use codemapx::tui::{self, App, Pane, keys::{self, Action}};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn app() -> App {
    App::new(common::sample_map(), PathBuf::from("/wt"))
}

fn at(app: &mut App, id: &str) {
    let i = app.map.card_index(id).unwrap();
    app.select(i);
}

fn key(app: &mut App, code: KeyCode) -> Action {
    keys::handle(app, KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn starts_at_first_trail_step_and_steps_in_order() {
    let mut a = app();
    assert_eq!(a.card().id, "src/billing/types.ts");
    key(&mut a, KeyCode::Right);
    key(&mut a, KeyCode::Right);
    assert_eq!(a.card().id, "src/billing/apply.ts");
    key(&mut a, KeyCode::Left);
    assert_eq!(a.card().id, "src/billing/mills.ts");
}

#[test]
fn came_from_and_leads_to_for_apply() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    let names = |v: Vec<(usize, usize)>, a: &App| v.into_iter().map(|(i, _)| a.map.cards[i].name.clone()).collect::<Vec<_>>();
    assert_eq!(names(a.links(true), &a), vec!["mills.ts", "types.ts", "fee-writer.ts"]);
    assert_eq!(names(a.links(false), &a), vec!["route.ts"]);
}

#[test]
fn enter_follows_the_selected_link() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    a.focus = Pane::To;
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.card().id, "src/api/route.ts");
}

#[test]
fn editor_targets_diff_and_outline_lines() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/billing/apply.ts"), 1)));
    a.focus = Pane::Inside;
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/billing/apply.ts"), 4)));
    at(&mut a, "enqueue");
    assert_eq!(a.editor_target(), None);
    at(&mut a, "src/api/legacy.ts");
    assert_eq!(a.editor_target(), None);
    at(&mut a, "src/jobs/fee-writer.ts");
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/jobs/fee-writer.ts"), 1)));
}

#[test]
fn q_quits_and_o_on_missing_flashes() {
    let mut a = app();
    at(&mut a, "enqueue");
    assert!(matches!(key(&mut a, KeyCode::Char('o')), Action::None));
    assert!(a.flash.is_some());
    assert!(matches!(key(&mut a, KeyCode::Char('q')), Action::Quit));
}

#[test]
fn snapshot_apply_180() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    let frame = tui::snapshot(&mut a, 180, 52);
    for needle in ["Apply regenerated fees", "SHARED CONTRACTS", "▶ apply.ts", "◂ mills.ts", "▸ route.ts", "applyChanges", "fee-writer.ts", "not built"] {
        assert!(frame.contains(needle), "missing {needle:?}\n{frame}");
    }
    common::assert_golden("tests/golden/apply-180.txt", &frame);
}

#[test]
fn editor_command_uses_template_then_editor_then_nvim() {
    assert_eq!(tui::editor_command("/a b.ts", 7, Some("code -g {path}:{line}"), Some("vim")), vec!["code", "-g", "/a b.ts:7"]);
    assert_eq!(tui::editor_command("/a.ts", 7, None, Some("hx")), vec!["hx", "+7", "/a.ts"]);
    assert_eq!(tui::editor_command("/a.ts", 7, None, Some("code -w")), vec!["code", "-w", "+7", "/a.ts"]);
    assert_eq!(tui::editor_command("/a.ts", 7, Some("  "), None), vec!["nvim", "+7", "/a.ts"]);
}

#[test]
fn editor_targets_link_evidence_in_link_panes() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    a.focus = Pane::From;
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/billing/apply.ts"), 5)));
    a.from.select(Some(2));
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/jobs/fee-writer.ts"), 3)));
}

#[test]
fn link_panes_show_evidence() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    let frame = tui::snapshot(&mut a, 180, 52);
    assert!(frame.contains("src/jobs/fee-writer.ts:3"), "{frame}");
}

#[test]
fn column_layout_starts_tests_and_docs_collapsed() {
    let names = ["Shared", "Apply", "Tests", "docs"];
    assert_eq!(column_layout(&names, 180, false), Some(vec![Expanded, Expanded, Collapsed, Collapsed]));
    assert_eq!(column_layout(&names, 180, true), Some(vec![Expanded; 4]));
    assert_eq!(column_layout(&names, 120, false), Some(vec![Expanded, Expanded, Collapsed, Collapsed]));
    assert_eq!(column_layout(&names, 120, true), Some(vec![Collapsed, Collapsed, Expanded, Expanded]));
    assert_eq!(column_layout(&["A", "B"], 120, false), Some(vec![Expanded; 2]));
    assert_eq!(column_layout(&names, 99, false), None);
}

#[test]
fn snapshot_apply_120_collapses() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    let frame = tui::snapshot(&mut a, 120, 52);
    assert!(frame.contains("TESTS · 1") && frame.contains("DOCS · 1"), "{frame}");
    common::assert_golden("tests/golden/apply-120.txt", &frame);
    // mills.test.ts sits in the collapsed Tests column and is a leads-to of mills.ts.
    at(&mut a, "src/billing/mills.ts");
    assert!(tui::snapshot(&mut a, 120, 52).contains("▸ 1 leads to"));
    assert!(tui::snapshot(&mut a, 120, 52).contains("TESTS · 1"));
    key(&mut a, KeyCode::Char('t'));
    let flipped = tui::snapshot(&mut a, 120, 52);
    assert!(!flipped.contains("TESTS · 1") && !flipped.contains("DOCS · 1"), "{flipped}");
    assert!(flipped.contains("APPLY · 2") && flipped.contains("TESTS  ") && flipped.contains("▸ mills.test.ts"), "{flipped}");
}

#[test]
fn too_narrow_says_so() {
    let mut a = app();
    assert!(tui::snapshot(&mut a, 90, 30).contains("terminal too narrow (need 100)"));
}

#[test]
fn banners_for_stale_maps() {
    let mut a = app();
    assert_eq!(a.banner(), None);
    a.behind = Some(2);
    assert_eq!(a.banner().as_deref(), Some("map is 2 commits behind HEAD — run /codemapx in the agent session"));
    a.behind = Some(0);
    assert_eq!(a.banner().as_deref(), Some("map was made for a different commit — run /codemapx in the agent session"));
    a.behind = None;
    a.map.annotations_stale = true;
    assert!(a.banner().unwrap().contains("/codemapx"));
    assert!(tui::snapshot(&mut a, 180, 52).lines().nth(1).unwrap().contains("/codemapx"));
}

#[test]
fn tall_columns_scroll_and_leave_room_for_the_diff() {
    let mut m = common::sample_map();
    let proto = m.cards[m.card_index("src/billing/apply.ts").unwrap()].clone();
    for k in 0..60 {
        let mut c = proto.clone();
        (c.id, c.name, c.column) = (format!("gen/f{k:02}.ts"), format!("f{k:02}.ts"), 0);
        m.columns[0].cards.push(m.cards.len());
        m.cards.push(c);
    }
    let mut a = App::new(m, PathBuf::from("/wt"));
    at(&mut a, "gen/f45.ts");
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("▶ f45.ts") && frame.contains("↑ ") && frame.contains("↓ "), "{frame}");
    let lines: Vec<&str> = frame.lines().collect();
    let top = lines.iter().position(|l| l.starts_with("╭ diff ·")).unwrap();
    let diff_rows = lines.len() - 2 - top - 1; // minus the help line and both borders
    assert!(diff_rows >= 15, "diff pane has {diff_rows} rows\n{frame}");
    // Columns without the selection show their top.
    assert!(frame.contains("APPLY") && frame.contains("apply.ts"), "{frame}");
}

#[test]
fn editor_spawn_failure_is_an_error() {
    let err = tui::open_editor(std::path::Path::new("/a.ts"), 3, Some("codemapx-no-such-editor {path}"), None).unwrap_err();
    assert!(err.contains("codemapx-no-such-editor"), "{err}");
}

#[test]
fn map_up_down_stays_in_column() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    key(&mut a, KeyCode::Down);
    assert_eq!(a.card().id, "src/billing/mills.ts");
    key(&mut a, KeyCode::Up);
    key(&mut a, KeyCode::Up);
    assert_eq!(a.card().id, "src/billing/types.ts");
}

#[test]
fn map_h_l_move_between_columns_keeping_row() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    key(&mut a, KeyCode::Char('l'));
    assert_eq!(a.card().id, "src/jobs/fee-writer.ts");
    key(&mut a, KeyCode::Char('l'));
    assert_eq!(a.card().id, "src/api/legacy.ts");
    key(&mut a, KeyCode::Char('l'));
    assert_eq!(a.card().id, "src/billing/mills.test.ts");
    key(&mut a, KeyCode::Char('h'));
    key(&mut a, KeyCode::Char('h'));
    key(&mut a, KeyCode::Char('h'));
    key(&mut a, KeyCode::Char('h'));
    assert_eq!(a.card().id, "src/billing/types.ts");
}

#[test]
fn c_toggles_the_selected_cards_column_and_shift_c_resets() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    assert!(tui::snapshot(&mut a, 180, 52).contains("mills.ts"));
    key(&mut a, KeyCode::Char('c'));
    let frame = tui::snapshot(&mut a, 180, 52);
    assert!(frame.contains("APPLY · 2"), "{frame}");
    assert!(frame.contains("▶ selected"));
    key(&mut a, KeyCode::Char('h'));
    key(&mut a, KeyCode::Char('c'));
    assert!(tui::snapshot(&mut a, 180, 52).contains("SHARED CONTRACT"));
    key(&mut a, KeyCode::Char('c'));
    assert!(!tui::snapshot(&mut a, 180, 52).contains("SHARED CONTRACTS · 2"));
    key(&mut a, KeyCode::Char('C'));
    assert!(!tui::snapshot(&mut a, 180, 52).contains("APPLY · 2"));
}

#[test]
fn c_expands_an_auto_collapsed_column_and_t_clears_it() {
    let mut a = app();
    at(&mut a, "docs/apply.md");
    assert!(tui::snapshot(&mut a, 120, 52).contains("DOCS · 1"));
    key(&mut a, KeyCode::Char('c'));
    assert!(!tui::snapshot(&mut a, 120, 52).contains("DOCS · 1"));
    key(&mut a, KeyCode::Char('t'));
    key(&mut a, KeyCode::Char('t'));
    assert!(tui::snapshot(&mut a, 120, 52).contains("DOCS · 1"));
}

fn mouse(app: &mut App, kind: MouseEventKind, row: u16) {
    tui::mouse::handle(app, MouseEvent { kind, column: 40, row, modifiers: KeyModifiers::NONE });
}

fn drag(app: &mut App, from: u16, to: u16) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
    tui::snapshot(app, 180, 50);
}

fn heights(app: &App) -> [u16; 3] {
    app.panes.map(|r| r.height)
}

#[test]
fn dragging_the_map_border_trades_rows_with_the_middle_row_only() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    let [map, mid, diff] = heights(&a);
    let y = a.panes[1].y;
    drag(&mut a, y, y + 3);
    assert_eq!(heights(&a), [map + 3, mid - 3, diff]);
    // The map's own bottom border is a handle too, and grabbing it does not jump.
    let y = a.panes[0].bottom() - 1;
    drag(&mut a, y, y - 2);
    assert_eq!(heights(&a), [map + 1, mid - 1, diff]);
}

#[test]
fn dragging_the_diff_border_trades_rows_with_the_middle_row_only() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    let [map, mid, diff] = heights(&a);
    let y = a.panes[2].y;
    drag(&mut a, y, y - 5);
    assert_eq!(heights(&a), [map, mid - 5, diff + 5]);
}

#[test]
fn drags_clamp_every_row_to_three_lines() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    let [map, mid, diff] = heights(&a);
    let y = a.panes[1].y;
    drag(&mut a, y, 0);
    assert_eq!(heights(&a), [3, map + mid - 3, diff]);
    let y = a.panes[2].y;
    drag(&mut a, y, 49);
    assert_eq!(heights(&a)[2], 3);
    // A shorter terminal squeezes the dragged rows before the diff.
    tui::snapshot(&mut a, 180, 20);
    assert_eq!(heights(&a), [3, 12, 3]);
}

#[test]
fn clicks_off_a_border_do_not_resize() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    let before = heights(&a);
    let y = a.panes[1].y + 2;
    drag(&mut a, y, y + 4);
    mouse(&mut a, MouseEventKind::Drag(MouseButton::Left), y + 6);
    tui::snapshot(&mut a, 180, 50);
    assert_eq!(heights(&a), before);
    assert!(a.heights.is_none());
}
