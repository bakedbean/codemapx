mod common;

use std::path::PathBuf;

use codemapx::tui::{self, App, Pane, keys::{self, Action}};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
