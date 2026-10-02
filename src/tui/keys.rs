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
        KeyCode::Tab => {
            let p = PANES.iter().position(|p| *p == app.focus).unwrap_or(0);
            app.focus = PANES[(p + 1) % PANES.len()];
        }
        KeyCode::BackTab => {
            let p = PANES.iter().position(|p| *p == app.focus).unwrap_or(0);
            app.focus = PANES[(p + PANES.len() - 1) % PANES.len()];
        }
        KeyCode::Char('h') if app.focus == Pane::Map => app.move_column(-1),
        KeyCode::Char('l') if app.focus == Pane::Map => app.move_column(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_in(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_in(1),
        KeyCode::Char('K') | KeyCode::PageUp => app.scroll_by(-15),
        KeyCode::Char('J') | KeyCode::PageDown => app.scroll_by(15),
        KeyCode::Enter => app.enter(),
        KeyCode::Char('t') => app.show_hidden = !app.show_hidden,
        KeyCode::Char('d') => app.diff_full = !app.diff_full,
        KeyCode::Char('o') => match app.editor_target() {
            Some((path, line)) => return Action::Open(path, line),
            None => app.flash = Some(" Nothing to open here.".into()),
        },
        _ => {}
    }
    Action::None
}
