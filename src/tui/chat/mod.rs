//! The agent chat panel: an agent's own TUI in a PTY beside the diff, briefed on the branch.

pub mod agent;
pub mod briefing;
pub mod keys;
pub mod pty;
pub mod render;

use std::{env, path::Path, sync::mpsc, time::Instant};

use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use super::{DIM, app::{App, Pane}, fns_pane::{self, MIN_DIFF_WIDTH}, pane_block};
use crate::{facts::Status, map::CardKind};
use agent::AgentKind;
use pty::Session;

/// Narrowest the panel gets, borders included.
pub(crate) const MIN_WIDTH: u16 = 40;

/// The panel's width out of `room` columns (the diff row less the minimap and the functions panel's minimum): 40% by default, leaving the diff `MIN_DIFF_WIDTH`.
pub(crate) fn width(want: Option<u16>, room: u16) -> u16 {
    want.unwrap_or(room * 2 / 5).clamp(MIN_WIDTH, room.saturating_sub(MIN_DIFF_WIDTH).max(MIN_WIDTH))
}

/// Columns the chat may share with the diff: the row less the minimap and, when shown, the functions panel's minimum.
pub(crate) fn room(row: u16, minimap: u16, fns_shown: bool) -> u16 {
    row - minimap - if fns_shown { fns_pane::MIN_WIDTH } else { 0 }
}

pub struct Chat {
    pub kind: AgentKind,
    pub session: Option<Session>,
    /// Why the agent couldn't start; shown in place of it.
    pub error: Option<String>,
    /// A reference waiting for the composer.
    pub queued: Option<String>,
    /// The last reference typed, so refocusing on the same lines doesn't repeat it.
    pub last_ref: Option<String>,
    wake_tx: mpsc::SyncSender<()>,
    wake: mpsc::Receiver<()>,
}

impl Chat {
    pub fn new(kind: AgentKind) -> Self {
        let (wake_tx, wake) = mpsc::sync_channel(1);
        Chat { kind, session: None, error: None, queued: None, last_ref: None, wake_tx, wake }
    }

    pub fn live(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.exit_code().is_none())
    }

    /// Replaces any earlier agent with a fresh one; a spawn failure is kept in `error`.
    pub fn start(&mut self, argv: &[String], cwd: &Path, rows: u16, cols: u16) {
        self.session = None;
        self.last_ref = None;
        match Session::spawn(argv, cwd, rows, cols, self.wake_tx.clone()) {
            Ok(s) => {
                self.session = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Writes to the agent and returns its view to the live screen; typing into a ready composer cancels a queued reference.
    /// Empty `bytes` (a swallowed key) do nothing.
    pub fn write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Some(s) = &mut self.session {
            if agent::ready(self.kind, s.parser().screen()) {
                self.queued = None;
            }
            s.scroll_to_live();
            s.write(bytes);
        }
    }

    /// Keeps only the newest reference, and none that was just typed.
    pub fn queue(&mut self, r: String) {
        self.queued = if self.last_ref.as_deref() == Some(r.as_str()) { None } else { Some(r) };
    }

    /// Types the queued reference, without Enter, once the agent is settled and its composer is up; leaving the chat drops it.
    pub fn tick(&mut self, now: Instant, focused: bool) {
        if !focused {
            self.queued = None;
            return;
        }
        let Some(r) = &self.queued else { return };
        let Some(s) = &mut self.session else { return };
        if !(s.settled(now) && agent::ready(self.kind, s.parser().screen())) {
            return;
        }
        let r = r.clone();
        if s.write(&keys::wrap_paste(&r)) {
            self.last_ref = Some(r);
            self.queued = None;
        }
    }

    /// True when the agent produced output since the last call.
    pub fn drain_wake(&self) -> bool {
        self.wake.try_iter().count() > 0
    }
}

pub(crate) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Chat;
    let mut title = match &app.chat {
        Some(c) => format!(" chat · {} ", c.kind.name()),
        None => " chat ".into(),
    };
    if focused {
        title.push_str("· ctrl-x leaves ");
    }
    let block = pane_block(Line::from(title), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let note = |f: &mut Frame, t: String| f.render_widget(Paragraph::new(Span::styled(t, Style::default().fg(DIM))).wrap(Wrap { trim: true }), inner);
    let Some(chat) = app.chat.as_mut() else { return note(f, "starting the agent…".into()) };
    if let Some(e) = &chat.error {
        return note(f, format!("{e} — ⏎ to retry"));
    }
    let Some(s) = chat.session.as_mut() else { return note(f, "starting the agent…".into()) };
    if let Some(code) = s.exit_code() {
        return note(f, format!("agent exited (code {code}) — ⏎ to restart"));
    }
    s.resize(inner.height.max(1), inner.width.max(1));
    let p = s.parser();
    let screen = p.screen();
    render::render_screen(screen, f.buffer_mut(), inner);
    if focused && screen.scrollback() == 0 && !screen.hide_cursor() {
        let (r, c) = screen.cursor_position();
        f.set_cursor_position((inner.x + c, inner.y + r));
    }
}

/// `path:start-end ` for what the reviewer was on in `from`, the pane focus came from: its function or outline entry,
/// else the highlighted diff line, else the visible lines. None when there's no file at HEAD to point at.
pub fn reference(app: &App, from: Pane) -> Option<String> {
    let c = app.card();
    if c.kind == CardKind::Missing || c.binary || c.status == Some(Status::Deleted) {
        return None;
    }
    let path = c.path.as_deref()?;
    let picked = match from {
        Pane::Functions => app.fns.selected().map(|s| (c.functions[s].start, c.functions[s].end)),
        Pane::Inside => app.inside.selected().map(|s| (c.outline[s].start, c.outline[s].end)),
        _ => None,
    };
    let range = picked.or_else(|| app.hl.and_then(|h| app.lines.get(h)?.n).map(|n| (n, n))).or_else(|| {
        let rows = app.panes[2].height.saturating_sub(2).max(1) as usize;
        let mut ns = app.lines.iter().skip(app.scroll).take(rows).filter_map(|l| l.n);
        let first = ns.next()?;
        Some((first, ns.next_back().unwrap_or(first)))
    });
    Some(match range {
        Some((a, b)) if a == b => format!("{path}:{a} "),
        Some((a, b)) => format!("{path}:{a}-{b} "),
        None => format!("{path} "),
    })
}

/// Starts the agent sized to the panel as last drawn, briefed on the map; CODEMAPX_AGENT picks it, CODEMAPX_AGENT_BIN its binary.
pub fn start(app: &mut App) {
    let inner = app.chat_pane.inner(Margin::new(1, 1));
    let brief = briefing::briefing(&app.map, app.map_dir.as_deref());
    let chat = app.chat.get_or_insert_with(|| Chat::new(AgentKind::Claude));
    match AgentKind::from_env(env::var("CODEMAPX_AGENT").ok().as_deref()) {
        Ok(k) => chat.kind = k,
        Err(e) => {
            chat.error = Some(e);
            return;
        }
    }
    let argv = agent::argv(chat.kind, env::var("CODEMAPX_AGENT_BIN").ok().as_deref(), &brief);
    chat.start(&argv, &app.root, inner.height.max(1), inner.width.max(1));
}
