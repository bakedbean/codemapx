//! Key handling, separated from the terminal so tests can drive it.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{app::{App, PANES, Pane}, chat};

pub enum Action {
    None,
    Quit,
    Open(PathBuf, usize),
    /// Start (or restart) the chat agent once the panel has been drawn at its size.
    StartChat,
}

/// A bracketed paste goes to a live agent in the focused chat; elsewhere it is ignored.
pub fn paste(app: &mut App, s: &str) {
    if app.focus == Pane::Chat
        && let Some(c) = app.chat.as_mut().filter(|c| c.live())
    {
        c.write(&chat::keys::wrap_paste(s));
    }
}

pub fn handle(app: &mut App, key: KeyEvent) -> Action {
    let prev = app.focus;
    let action = dispatch(app, key);
    app.note_focus(prev);
    action
}

fn dispatch(app: &mut App, key: KeyEvent) -> Action {
    app.flash = None;
    if app.focus == Pane::Chat
        && let Some(a) = chat_key(app, key)
    {
        return a;
    }
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
        KeyCode::Char('a') if app.toggle_chat() => return Action::StartChat,
        KeyCode::Char('a') => {}
        KeyCode::Char('o') => match app.editor_target() {
            Some((path, line)) => return Action::Open(path, line),
            None => app.flash = Some(" Nothing to open here.".into()),
        },
        _ => {}
    }
    Action::None
}

/// Moves focus `by` panes forward, skipping the functions and chat panels when they aren't on screen.
fn cycle(app: &mut App, by: usize) {
    let mut p = PANES.iter().position(|p| *p == app.focus).unwrap_or(0);
    loop {
        p = (p + by) % PANES.len();
        let hidden = (PANES[p] == Pane::Functions && app.fns_pane.width == 0) || (PANES[p] == Pane::Chat && app.chat_pane.width == 0);
        if !hidden {
            break;
        }
    }
    app.focus = PANES[p];
}

/// The focused chat sends a live agent every key but ctrl-x; with no agent, ⏎ starts one and other keys fall through (None).
fn chat_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.focus = Pane::Diff;
        return Some(Action::None);
    }
    match app.chat.as_mut() {
        Some(c) if c.live() => {
            c.write(&chat::keys::encode_key(key));
            Some(Action::None)
        }
        _ if key.code == KeyCode::Enter => Some(Action::StartChat),
        _ => None,
    }
}
