//! Terminal view of a merged map.

pub mod app;
mod diff_pane;
pub mod keys;
mod link_panes;
mod map_pane;

use std::{env, io, path::Path, process::Command};

use ratatui::{
    backend::TestBackend,
    crossterm::event::{self, Event, KeyEventKind},
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub use app::{App, Pane};
use keys::Action;



pub(crate) const AMBER: Color = Color::Rgb(224, 177, 87);
pub(crate) const BLUE: Color = Color::Rgb(132, 162, 255);
pub(crate) const GREEN: Color = Color::Rgb(111, 207, 127);
pub(crate) const RED: Color = Color::Rgb(255, 138, 138);
pub(crate) const DIM: Color = Color::Rgb(110, 118, 132);
pub(crate) const FAINT: Color = Color::Rgb(70, 76, 88);
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
    let map_h = app.map.columns.iter().map(|c| c.cards.len()).max().unwrap_or(0) as u16 + 3;
    let rows = if app.diff_full {
        Layout::vertical([Constraint::Length(1), Constraint::Length(0), Constraint::Length(0), Constraint::Min(3), Constraint::Length(1)]).split(area)
    } else {
        Layout::vertical([Constraint::Length(1), Constraint::Length(map_h), Constraint::Length(16), Constraint::Min(5), Constraint::Length(1)]).split(area)
    };
    draw_header(f, app, rows[0]);
    if !app.diff_full {
        map_pane::draw(f, app, rows[1]);
        link_panes::draw(f, app, rows[2]);
    }
    diff_pane::draw(f, app, rows[3]);
    let help = app.flash.clone().unwrap_or_else(|| {
        " ←/→ step   tab pane   ↑/↓ move   ⏎ follow   o open in $EDITOR   d full diff   J/K page   q quit".into()
    });
    f.render_widget(Paragraph::new(Span::styled(help, Style::default().fg(DIM))), rows[4]);
}

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
    let mut term = ratatui::init();
    let res = event_loop(&mut term, app);
    ratatui::restore();
    res
}

fn event_loop(term: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        term.draw(|f| draw(f, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match keys::handle(app, key) {
            Action::Quit => return Ok(()),
            Action::Open(path, line) => {
                ratatui::restore();
                open_editor(&path, line);
                *term = ratatui::init();
                term.clear()?;
            }
            Action::None => {}
        }
    }
}

fn open_editor(path: &Path, line: usize) {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".into());
    let _ = Command::new(editor).arg(format!("+{line}")).arg(path).status();
}
