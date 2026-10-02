//! Key handling, separated from the terminal so tests can drive it.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, PANES, Pane};

pub enum Action {
    None,
    Quit,
    Open(PathBuf, usize),
}

pub fn handle(app: &mut App, key: KeyEvent) -> Action {
    app.flash = None;
    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
        KeyCode::Esc if app.diff_full => app.diff_full = false,
        KeyCode::Esc => return Action::Quit,
        KeyCode::Left => app.step(-1),
        KeyCode::Right => app.step(1),
        KeyCode::Tab => cycle(app, 1),
        KeyCode::BackTab => cycle(app, PANES.len() - 1),
        KeyCode::Char('h') if app.focus == Pane::Map => app.move_column(-1),
        KeyCode::Char('l') if app.focus == Pane::Map => app.move_column(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_in(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_in(1),
        KeyCode::Char('K') | KeyCode::PageUp => app.scroll_by(-15),
        KeyCode::Char('J') | KeyCode::PageDown => app.scroll_by(15),
        KeyCode::Enter => app.enter(),
        KeyCode::Char('t') => app.toggle_hidden(),
        KeyCode::Char('c') => app.toggle_column(),
        KeyCode::Char('C') => app.col_overrides.fill(None),
        KeyCode::Char('d') => app.diff_full = !app.diff_full,
        KeyCode::Char('f') => app.toggle_fns(),
        KeyCode::Char('m') => app.show_minimap = !app.show_minimap,
        KeyCode::Char('o') => match app.editor_target() {
            Some((path, line)) => return Action::Open(path, line),
            None => app.flash = Some(" Nothing to open here.".into()),
        },
        _ => {}
    }
    Action::None
}

/// Moves focus `by` panes forward, skipping the functions panel when it isn't on screen.
fn cycle(app: &mut App, by: usize) {
    let mut p = PANES.iter().position(|p| *p == app.focus).unwrap_or(0);
    loop {
        p = (p + by) % PANES.len();
        if PANES[p] != Pane::Functions || app.fns_pane.width > 0 {
            break;
        }
    }
    app.focus = PANES[p];
}
