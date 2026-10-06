//! Terminal view of a merged map.

pub mod app;
pub mod chat;
mod diff_pane;
mod fns_pane;
pub mod keys;
mod link_panes;
pub mod map_pane;
mod minimap;
pub mod mouse;

use std::{env, io, path::Path, process::Command, time::{Duration, Instant}};

use ratatui::{
    backend::TestBackend,
    crossterm::{
        event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind},
        execute,
    },
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub use app::{App, Pane};
pub use map_pane::{ColumnView, column_layout};
use keys::Action;



pub(crate) const AMBER: Color = Color::Rgb(224, 177, 87);
pub(crate) const BLUE: Color = Color::Rgb(132, 162, 255);
pub(crate) const GREEN: Color = Color::Rgb(111, 207, 127);
pub(crate) const RED: Color = Color::Rgb(255, 138, 138);
pub(crate) const DIM: Color = Color::Rgb(130, 138, 152);
pub(crate) const FAINT: Color = Color::Rgb(98, 105, 120);
pub(crate) const SEL_BG: Color = Color::Rgb(38, 44, 58);
pub(crate) const HL_BG: Color = Color::Rgb(48, 58, 92);

pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

pub(crate) fn trunc(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.width() + 2 > w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Keeps the end of a file name, which is what tells siblings apart.
pub(crate) fn trunc_left(s: &str, w: usize) -> String {
    let s = s.trim_start_matches('…');
    if s.width() <= w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for c in chars.iter().rev() {
        if out.width() + 2 > w {
            break;
        }
        out.insert(0, *c);
    }
    format!("…{out}")
}

pub(crate) fn pane_block(title: Line<'static>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(if focused { Color::White } else { FAINT }))
        .title(title)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.fns_pane = Rect::default();
    app.minimap = Rect::default();
    app.chat_pane = Rect::default();
    if area.width < map_pane::MIN_WIDTH {
        if matches!(app.focus, Pane::Functions | Pane::Chat) {
            app.focus = Pane::Diff;
        }
        f.render_widget(Paragraph::new(Span::styled("terminal too narrow (need 100)", Style::default().fg(DIM))), area);
        return;
    }
    let banner = app.banner();
    let banner_h = if banner.is_some() { 1 } else { 0 };
    let (map_h, mid_h) = row_heights(app, area.height.saturating_sub(2 + banner_h));
    let (map_c, mid_c) = if app.diff_full { (0, 0) } else { (map_h, mid_h) };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(banner_h),
        Constraint::Length(map_c),
        Constraint::Length(mid_c),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);
    app.panes = [rows[2], rows[3], rows[4]];
    app.mid_panes = [Rect::default(); 3];
    draw_header(f, app, rows[0]);
    if let Some(b) = &banner {
        f.render_widget(Paragraph::new(Span::styled(format!(" {b}"), Style::default().fg(AMBER).bold())), rows[1]);
    }
    if !app.diff_full {
        app.map_width = rows[2].width;
        map_pane::draw(f, app, rows[2]);
        link_panes::draw(f, app, rows[3]);
    }
    let row = rows[4];
    let fns_on = app.show_fns && area.width >= fns_pane::MIN_TERM_WIDTH;
    let reserve = (if app.show_chat { chat::MIN_WIDTH } else { 0 }) + (if fns_on { fns_pane::MIN_WIDTH } else { 0 });
    let mm_w = if app.show_minimap { minimap::width(app.minimap_width, row.width.saturating_sub(reserve)) } else { 0 };
    let chat_w = if app.show_chat { chat::width(app.chat_width, chat::room(row.width, mm_w, fns_on)) } else { 0 };
    let fns_w = if fns_on { fns_pane::width(app.fns_width, row.width - mm_w - chat_w) } else { 0 };
    if fns_w == 0 && app.focus == Pane::Functions {
        app.focus = Pane::Diff;
    }
    let [fns, diff, chat_area, mm] = Layout::horizontal([Constraint::Length(fns_w), Constraint::Min(0), Constraint::Length(chat_w), Constraint::Length(mm_w)]).areas(row);
    if fns_w > 0 {
        app.fns_pane = fns;
        fns_pane::draw(f, app, fns);
    }
    if mm_w > 0 {
        app.minimap = mm;
        minimap::draw(f, app, mm, diff.height.saturating_sub(2) as usize);
    }
    if chat_w > 0 {
        app.chat_pane = chat_area;
        chat::draw(f, app, chat_area);
    }
    diff_pane::draw(f, app, diff);
    let help = app.flash.clone().unwrap_or_else(|| {
        " ←/→ step  tab pane  ↑/↓ move  h/l col  c fold  ⏎ follow  o open  d full  f fns  m minimap  a chat  J/K page  t tests  q quit".into()
    });
    f.render_widget(Paragraph::new(Span::styled(help, Style::default().fg(DIM))), rows[5]);
}

/// Map and middle-row heights out of `avail` rows; dragged heights shrink to keep every row at least `MIN_ROW_H`.
fn row_heights(app: &App, avail: u16) -> (u16, u16) {
    const MID_H: u16 = 16;
    if let Some((m, d)) = app.heights {
        let m = m.clamp(MIN_ROW_H, avail.saturating_sub(2 * MIN_ROW_H).max(MIN_ROW_H));
        return (m, d.clamp(MIN_ROW_H, avail.saturating_sub(m + MIN_ROW_H).max(MIN_ROW_H)));
    }
    // The map gets at most 40% of the rows it shares with the diff (min: header + 3 cards); columns scroll.
    let shared = avail.saturating_sub(MID_H);
    let map_h = (app.map.columns.iter().map(|c| c.cards.len()).max().unwrap_or(0) as u16 + 3).min((shared * 2 / 5).max(6));
    (map_h, MID_H)
}

/// Borders plus one line of content.
pub(crate) const MIN_ROW_H: u16 = 3;

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let m = &app.map;
    let step = match m.trail.iter().position(|&t| t == app.cur) {
        Some(p) => format!("step {}/{}", p + 1, m.trail.len()),
        None => match app.card().kind {
            crate::map::CardKind::Missing => "not built".into(),
            _ => "context".into(),
        },
    };
    let eyebrow = format!("{} · {} · {} commits · {} files · +{} −{}", m.repo, m.branch, m.commits, m.files, m.add, m.del);
    let left = format!(" {}  ", m.title);
    let right = format!("{step} ");
    let mid = trunc(&eyebrow, (area.width as usize).saturating_sub(left.width() + right.width() + 1));
    let pad = (area.width as usize).saturating_sub(left.width() + mid.width() + right.width());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, Style::default().bold()),
            Span::styled(mid, Style::default().fg(DIM)),
            Span::raw(" ".repeat(pad)),
            Span::styled(right, Style::default().fg(BLUE)),
        ])),
        area,
    );
}

/// One frame as text, for tests and `view --snapshot`.
pub fn snapshot(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    term.draw(|f| draw(f, app)).expect("draw");
    let buf = term.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn run(app: &mut App) -> io::Result<()> {
    let mut term = init()?;
    let res = event_loop(&mut term, app);
    restore();
    res
}

/// Mouse capture is for border drags and wheel-scrolling the panes; terminals still select text with shift/option-drag. Bracketed paste lets a paste reach the chat agent whole.
fn init() -> io::Result<ratatui::DefaultTerminal> {
    let term = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    Ok(term)
}

fn restore() {
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    ratatui::restore();
}

/// Blocks on input until an agent is running; then polls every 16 ms and redraws only on input or agent output.
fn event_loop(term: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut dirty = true;
    // Whether the last pass saw a live agent, so the pass after it exits draws the exit before input blocks.
    let mut was_live = false;
    loop {
        if dirty {
            term.draw(|f| draw(f, app))?;
        }
        dirty = true;
        let focused = app.focus == Pane::Chat;
        let shown = app.chat_pane.width > 0;
        if let Some(c) = app.chat.as_mut().filter(|c| c.live()) {
            c.tick(Instant::now(), focused);
            let woke = c.drain_wake();
            was_live = true;
            if !event::poll(Duration::from_millis(16))? {
                dirty = woke && shown;
                continue;
            }
        } else if was_live {
            was_live = false;
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            Event::Mouse(m) => {
                mouse::handle(app, m);
                continue;
            }
            Event::Paste(s) => {
                keys::paste(app, &s);
                continue;
            }
            _ => continue,
        };
        match keys::handle(app, key) {
            Action::Quit => return Ok(()),
            Action::StartChat => {
                // Draw first so the panel's size is known.
                term.draw(|f| draw(f, app))?;
                chat::start(app);
            }
            Action::Open(path, line) => {
                restore();
                let (t, e) = (env::var("CODEMAPX_EDITOR").ok(), env::var("EDITOR").ok());
                let res = open_editor(&path, line, t.as_deref(), e.as_deref());
                *term = init()?;
                term.clear()?;
                if let Err(e) = res {
                    app.flash = Some(format!(" {e}"));
                }
            }
            Action::None => {}
        }
    }
}

/// argv for opening `path` at `line`: the CODEMAPX_EDITOR template if set, else `$EDITOR +line path`.
pub fn editor_command(path: &str, line: usize, template: Option<&str>, editor: Option<&str>) -> Vec<String> {
    if let Some(t) = template.filter(|t| !t.trim().is_empty()) {
        return t.split_whitespace().map(|w| w.replace("{path}", path).replace("{line}", &line.to_string())).collect();
    }
    let mut argv: Vec<String> = editor.filter(|e| !e.trim().is_empty()).unwrap_or("nvim").split_whitespace().map(String::from).collect();
    argv.push(format!("+{line}"));
    argv.push(path.to_string());
    argv
}

/// Runs the editor and waits; Err names the command when it can't be started.
pub fn open_editor(path: &Path, line: usize, template: Option<&str>, editor: Option<&str>) -> Result<(), String> {
    let argv = editor_command(&path.to_string_lossy(), line, template, editor);
    Command::new(&argv[0]).args(&argv[1..]).status().map(|_| ()).map_err(|e| format!("can't run {}: {e}", argv[0]))
}
