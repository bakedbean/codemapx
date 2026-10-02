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
    let top = lines.iter().position(|l| l.contains("╭ diff ·")).unwrap();
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
    mouse_at(app, kind, 40, row);
}

fn mouse_at(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    tui::mouse::handle(app, MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
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

#[test]
fn the_wheel_scrolls_the_diff_three_lines_a_notch() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    let diff = a.panes[2].y + 2;
    mouse(&mut a, MouseEventKind::ScrollDown, diff);
    mouse(&mut a, MouseEventKind::ScrollDown, diff);
    assert_eq!(a.scroll, 6);
    mouse(&mut a, MouseEventKind::ScrollUp, diff);
    assert_eq!(a.scroll, 3);
}

#[test]
fn the_wheel_over_the_map_moves_the_selection_in_its_column() {
    let mut a = app();
    let col = a.map.columns.iter().find(|c| c.cards.len() > 1).unwrap().cards.clone();
    a.select(col[0]);
    tui::snapshot(&mut a, 180, 50);
    let map = a.panes[0].y + 2;
    mouse(&mut a, MouseEventKind::ScrollDown, map);
    assert_eq!(a.cur, col[1]);
    mouse(&mut a, MouseEventKind::ScrollUp, map);
    assert_eq!(a.cur, col[0]);
    assert_eq!(a.focus, Pane::Map);
}

#[test]
fn the_wheel_moves_the_middle_pane_under_it_without_taking_focus() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    tui::snapshot(&mut a, 180, 50);
    let [from, inside, to] = a.mid_panes;
    let y = a.panes[1].y + 2;
    mouse_at(&mut a, MouseEventKind::ScrollDown, from.x + 2, y);
    assert_eq!(a.from.selected(), Some(1));
    assert_eq!((a.to.selected(), a.inside.selected()), (Some(0), Some(0)));
    mouse_at(&mut a, MouseEventKind::ScrollDown, to.x + 2, y);
    assert_eq!(a.to.selected(), Some(0), "leads-to has one link");
    assert!(a.hl.is_none());
    mouse_at(&mut a, MouseEventKind::ScrollDown, inside.x + 2, y);
    assert_eq!(a.inside.selected(), Some(0), "apply.ts has one outline entry");
    assert!(a.hl.is_some(), "moving the outline jumps the diff");
    assert_eq!(a.focus, Pane::Map);
    assert_eq!(a.card().id, "src/billing/apply.ts");
}

#[test]
fn the_wheel_does_nothing_over_hidden_panes_in_full_diff() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    key(&mut a, KeyCode::Char('d'));
    tui::snapshot(&mut a, 180, 50);
    assert_eq!(a.mid_panes, [ratatui::layout::Rect::default(); 3]);
    let diff = a.panes[2].y + 2;
    mouse(&mut a, MouseEventKind::ScrollDown, diff);
    assert_eq!(a.scroll, 3);
    assert_eq!(a.card().id, "src/billing/apply.ts");
}

#[test]
fn functions_panel_lists_every_function_and_marks_changed_ones() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    let frame = tui::snapshot(&mut a, 180, 52);
    assert!(frame.contains("╭ functions "), "{frame}");
    assert!(frame.contains("│     1 toMills"), "{frame}");
    assert!(frame.contains("│ +   5 millsToDecimal"), "{frame}");
    assert!(a.fns_pane.width > 0 && a.fns_pane.x == 0);
}

#[test]
fn f_toggles_the_functions_panel_and_narrow_terminals_hide_it() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    key(&mut a, KeyCode::Char('f'));
    assert!(!tui::snapshot(&mut a, 180, 52).contains("╭ functions "));
    assert_eq!(a.fns_pane, ratatui::layout::Rect::default());
    key(&mut a, KeyCode::Char('f'));
    assert!(tui::snapshot(&mut a, 180, 52).contains("╭ functions "));
    assert!(!tui::snapshot(&mut a, 120, 52).contains("╭ functions "));
    key(&mut a, KeyCode::Char('d'));
    assert!(tui::snapshot(&mut a, 180, 52).contains("╭ functions "), "shown in full diff too");
}

#[test]
fn tab_reaches_the_functions_panel_only_while_shown() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 52);
    a.focus = Pane::To;
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.focus, Pane::Functions);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.focus, Pane::Diff);
    key(&mut a, KeyCode::BackTab);
    key(&mut a, KeyCode::Char('f'));
    assert_eq!(a.focus, Pane::Diff, "hiding the focused panel focuses the diff");
    tui::snapshot(&mut a, 180, 52);
    a.focus = Pane::To;
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.focus, Pane::Diff);
}

#[test]
fn moving_in_the_functions_panel_jumps_the_diff_and_sets_the_editor_target() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 52);
    a.focus = Pane::Functions;
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/billing/mills.ts"), 1)));
    key(&mut a, KeyCode::Down);
    assert_eq!(a.fns.selected(), Some(1));
    assert_eq!(a.hl.map(|h| a.lines[h].n), Some(Some(5)));
    assert_eq!(a.editor_target(), Some((PathBuf::from("/wt/src/billing/mills.ts"), 5)));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.focus, Pane::Diff);
}

#[test]
fn a_function_outside_the_diff_clears_the_highlight() {
    let mut a = app();
    let i = a.map.card_index("src/billing/mills.ts").unwrap();
    let mut far = a.map.cards[i].functions[0].clone();
    (far.name, far.start, far.end) = ("far".into(), 40, 44);
    a.map.cards[i].functions.push(far);
    a.select(i);
    a.move_pane(Pane::Functions, 1);
    assert!(a.hl.is_some());
    a.move_pane(Pane::Functions, 1);
    assert_eq!(a.fns.selected(), Some(2));
    assert!(a.hl.is_none());
}

#[test]
fn the_wheel_moves_the_functions_panel() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 52);
    let p = a.fns_pane;
    mouse_at(&mut a, MouseEventKind::ScrollDown, p.x + 2, p.y + 2);
    assert_eq!(a.fns.selected(), Some(1));
    assert!(a.hl.is_some(), "moving a function jumps the diff");
    assert_eq!(a.focus, Pane::Map);
}

#[test]
fn functions_panel_explains_empty_lists() {
    let mut a = app();
    at(&mut a, "docs/apply.md");
    assert!(tui::snapshot(&mut a, 180, 52).contains("TS/JS only"));
    at(&mut a, "src/billing/types.ts");
    assert!(tui::snapshot(&mut a, 180, 52).contains("No functions."));
    let i = a.map.card_index("src/billing/mills.ts").unwrap();
    a.map.cards[i].functions.clear();
    a.select(i);
    assert!(tui::snapshot(&mut a, 180, 52).contains("re-run /codemapx"));
}

#[test]
fn too_narrow_drops_the_functions_panel_and_its_focus() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 52);
    a.focus = Pane::Functions;
    tui::snapshot(&mut a, 80, 52);
    assert_eq!(a.fns_pane, ratatui::layout::Rect::default());
    assert_eq!(a.focus, Pane::Diff);
}

/// Drags along `row` from column `from` to `to`, then redraws.
fn drag_across(app: &mut App, row: u16, from: u16, to: u16) {
    mouse_at(app, MouseEventKind::Down(MouseButton::Left), from, row);
    mouse_at(app, MouseEventKind::Drag(MouseButton::Left), to, row);
    mouse_at(app, MouseEventKind::Up(MouseButton::Left), to, row);
    tui::snapshot(app, 180, 50);
}

fn fns_app() -> App {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 50);
    a
}

#[test]
fn dragging_the_functions_border_widens_and_narrows_the_panel() {
    let mut a = fns_app();
    let (p, diff) = (a.fns_pane, a.panes[2]);
    let row = p.y + 2;
    drag_across(&mut a, row, p.right() - 1, p.right() + 9);
    assert_eq!(a.fns_pane.width, p.width + 10);
    assert_eq!(a.panes[2], diff, "the diff row itself does not move");
    // The diff's own left border is a handle too, and grabbing it does not jump.
    let x = a.fns_pane.right();
    drag_across(&mut a, row, x, x - 6);
    assert_eq!(a.fns_pane.width, p.width + 4);
}

#[test]
fn functions_drags_clamp_the_panel_and_leave_the_diff_forty_columns() {
    let mut a = fns_app();
    let row = a.fns_pane.y + 2;
    let x = a.fns_pane.right() - 1;
    drag_across(&mut a, row, x, 0);
    assert_eq!(a.fns_pane.width, 16);
    let x = a.fns_pane.right() - 1;
    drag_across(&mut a, row, x, 179);
    assert_eq!(a.fns_pane.width, 120, "180 minus the 20-column minimap and 40 for the diff");
    // A narrower terminal squeezes the dragged panel before the diff.
    tui::snapshot(&mut a, 150, 50);
    assert_eq!(a.fns_pane.width, 90);
}

#[test]
fn clicks_off_the_functions_border_do_not_resize_it() {
    let mut a = fns_app();
    let p = a.fns_pane;
    drag_across(&mut a, p.y + 2, p.right() + 2, p.right() + 8);
    drag_across(&mut a, p.y + 2, p.right() - 3, p.right() + 8);
    assert_eq!(a.fns_pane, p);
    assert!(a.fns_width.is_none());
    // On the diff's top border the row divider wins.
    let [_, mid, _] = heights(&a);
    drag_across(&mut a, p.y, p.right() - 1, p.right() + 8);
    assert_eq!(a.fns_pane.width, p.width);
    assert_eq!(heights(&a)[1], mid);
    assert!(a.drag.is_none());
}

#[test]
fn a_hidden_functions_panel_has_no_border_to_grab() {
    let mut a = fns_app();
    let p = a.fns_pane;
    key(&mut a, KeyCode::Char('f'));
    tui::snapshot(&mut a, 180, 50);
    drag_across(&mut a, p.y + 2, p.right() - 1, p.right() + 9);
    assert!(a.fns_width.is_none());
}

#[test]
fn the_dragged_functions_width_survives_f_and_card_changes() {
    let mut a = fns_app();
    let p = a.fns_pane;
    drag_across(&mut a, p.y + 2, p.right() - 1, p.right() + 9);
    key(&mut a, KeyCode::Char('f'));
    key(&mut a, KeyCode::Char('f'));
    at(&mut a, "src/billing/apply.ts");
    tui::snapshot(&mut a, 180, 50);
    assert_eq!(a.fns_pane.width, p.width + 10);
}

#[test]
fn the_functions_border_drags_in_full_diff() {
    let mut a = fns_app();
    key(&mut a, KeyCode::Char('d'));
    tui::snapshot(&mut a, 180, 50);
    let p = a.fns_pane;
    drag_across(&mut a, p.y + 2, p.right() - 1, p.right() + 4);
    assert_eq!(a.fns_pane.width, p.width + 5);
}

#[test]
fn the_diff_shows_the_whole_file_with_changes_in_place() {
    let mut m = common::sample_map();
    let i = m.card_index("src/billing/mills.ts").unwrap();
    let src: String = (1..=20).map(|n| format!("l{n}\n")).collect::<String>().replace("l10\n", "L10\n");
    m.cards[i].source = Some(src);
    m.cards[i].diff = "@@ -2,2 +2,1 @@\n l2\n-gone\n@@ -11,1 +10,1 @@\n-l10\n+L10".into();
    let mut a = App::new(m, PathBuf::from("/wt"));
    a.select(i);
    let lines: Vec<(Option<usize>, String)> = a.lines.iter().map(|l| (l.n, l.text.clone())).collect();
    assert_eq!(lines.len(), 22, "20 lines plus two deletions");
    assert_eq!(lines[0], (Some(1), " l1".into()));
    assert_eq!(lines[2], (None, "-gone".into()));
    assert_eq!(lines[3], (Some(3), " l3".into()));
    assert_eq!(lines[10], (None, "-l10".into()));
    assert_eq!(lines[11], (Some(10), "+L10".into()));
    assert_eq!(lines[21], (Some(20), " l20".into()));
    assert!(lines.iter().all(|(_, t)| !t.starts_with("@@")));
}

#[test]
fn the_minimap_sits_right_of_the_diff_and_m_toggles_it() {
    let mut a = fns_app();
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("╭ minimap "), "{frame}");
    assert_eq!((a.minimap.right(), a.minimap.width), (180, 20));
    key(&mut a, KeyCode::Char('m'));
    assert!(!tui::snapshot(&mut a, 180, 50).contains("╭ minimap "));
    assert_eq!(a.minimap, ratatui::layout::Rect::default());
    key(&mut a, KeyCode::Char('m'));
    assert!(tui::snapshot(&mut a, 180, 50).contains("╭ minimap "));
}

#[test]
fn dragging_the_minimap_border_resizes_it_and_leaves_the_diff_forty_columns() {
    let mut a = fns_app();
    let m = a.minimap;
    let row = m.y + 2;
    drag_across(&mut a, row, m.x, m.x - 10);
    assert_eq!(a.minimap.width, 30);
    // The diff's own right border is a handle too.
    let x = a.minimap.x - 1;
    drag_across(&mut a, row, x, x + 4);
    assert_eq!(a.minimap.width, 26);
    let x = a.minimap.x;
    drag_across(&mut a, row, x, 0);
    assert_eq!(a.minimap.width, 140);
    let x = a.minimap.x;
    drag_across(&mut a, row, x, 179);
    assert_eq!(a.minimap.width, 8);
}

fn long_file_app() -> App {
    let mut m = common::sample_map();
    let i = m.card_index("src/billing/mills.ts").unwrap();
    let src: String = (1..=400).map(|n| format!("line {n}\n")).collect::<String>().replace("line 300\n", "LINE 300\n");
    m.cards[i].source = Some(src);
    m.cards[i].diff = "@@ -300,1 +300,1 @@\n-line 300\n+LINE 300".into();
    let mut a = App::new(m, PathBuf::from("/wt"));
    a.select(i);
    tui::snapshot(&mut a, 180, 50);
    a
}

#[test]
fn clicking_the_minimap_centres_the_diff_there_and_the_wheel_scrolls_it() {
    let mut a = long_file_app();
    let m = a.minimap;
    let bottom = m.bottom() - 2;
    mouse_at(&mut a, MouseEventKind::Down(MouseButton::Left), m.x + 3, bottom);
    mouse_at(&mut a, MouseEventKind::Up(MouseButton::Left), m.x + 3, bottom);
    assert!(a.scroll > 300, "jumped near the end, got {}", a.scroll);
    mouse_at(&mut a, MouseEventKind::Down(MouseButton::Left), m.x + 3, m.y + 1);
    assert_eq!(a.scroll, 0);
    mouse_at(&mut a, MouseEventKind::ScrollDown, m.x + 3, m.y + 4);
    assert_eq!(a.scroll, 3);
    assert!(a.drag.is_none() && a.minimap_width.is_none());
}

#[test]
fn the_minimap_colours_the_changed_rows() {
    let mut a = long_file_app();
    let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(180, 50)).unwrap();
    term.draw(|f| tui::draw(f, &mut a)).unwrap();
    let m = a.minimap;
    let buf = term.backend().buffer();
    let green = |y: u16| (m.x + 1..m.right() - 1).any(|x| [buf[(x, y)].fg, buf[(x, y)].bg].contains(&ratatui::style::Color::Rgb(111, 207, 127)));
    let rows: Vec<u16> = (m.y + 1..m.bottom() - 1).filter(|&y| green(y)).collect();
    assert_eq!(rows.len(), 1, "one changed row, got {rows:?}");
    // Four lines a row leaves the map shorter than the panel, so measure against the drawn rows.
    let drawn = (m.y + 1..m.bottom() - 1).filter(|&y| (m.x + 1..m.right() - 1).any(|x| buf[(x, y)].symbol() != " ")).count();
    let frac = (rows[0] - m.y - 1) as f32 / drawn as f32;
    assert!((0.65..0.8).contains(&frac), "change about 3/4 down, at {frac}");
}
