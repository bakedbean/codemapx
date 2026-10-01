use ratatui::{prelude::*, widgets::Paragraph};

use super::{DIM, FAINT, GREEN, HL_BG, RED, app::{App, Kind, Pane}, pane_block};
use crate::map::CardKind;

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    let card = app.card();
    let title = match card.kind {
        CardKind::Changed => format!(" diff · {} ", card.path.as_deref().unwrap_or("")),
        CardKind::Context => " diff · unchanged ".to_string(),
        CardKind::Missing => " not built ".to_string(),
    };
    let block = pane_block(Line::from(title), app.focus == Pane::Diff);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<Line> = app
        .lines
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(inner.height as usize)
        .map(|(i, l)| {
            let (fg, gutter) = match l.kind {
                Kind::Add => (GREEN, l.n.map(|n| format!("{n:>4} ")).unwrap_or_default()),
                Kind::Del => (RED, "     ".into()),
                Kind::Hunk => (DIM, "     ".into()),
                Kind::Ctx => (Color::Reset, l.n.map(|n| format!("{n:>4} ")).unwrap_or_default()),
                Kind::Note => (DIM, String::new()),
            };
            let near_hl = app.hl.is_some_and(|h| i >= h && i < h + 3);
            let style = if near_hl { Style::default().bg(HL_BG) } else { Style::default() };
            Line::from(vec![Span::styled(gutter, Style::default().fg(FAINT)), Span::styled(l.text.replace('\t', "    "), Style::default().fg(fg))]).style(style)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
