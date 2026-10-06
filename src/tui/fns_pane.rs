use ratatui::{
    prelude::*,
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{DIM, FAINT, GREEN, SEL_BG, app::{App, Pane}, pane_block, trunc};
use crate::{collect::outline::lang_for, map::CardKind};

/// Columns the panel takes by default, borders included.
pub(super) const WIDTH: u16 = 32;
/// Narrowest a drag can make the panel.
pub(super) const MIN_WIDTH: u16 = 16;
/// Columns a drag must leave the diff.
pub(super) const MIN_DIFF_WIDTH: u16 = 40;
/// Narrowest terminal that still shows the panel beside the diff.
pub(super) const MIN_TERM_WIDTH: u16 = 130;

/// The panel's width out of a `row` columns wide, clamped so the diff keeps `MIN_DIFF_WIDTH`.
pub(super) fn width(want: Option<u16>, row: u16) -> u16 {
    want.unwrap_or(WIDTH).clamp(MIN_WIDTH, row.saturating_sub(MIN_DIFF_WIDTH).max(MIN_WIDTH))
}

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = pane_block(Line::from(" functions "), app.focus == Pane::Functions);
    let inner = block.inner(area);
    let card = app.card();
    if card.functions.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(empty_reason(app), Style::default().fg(DIM))).block(block), area);
        return;
    }
    let w = inner.width as usize;
    let items: Vec<ListItem> = card
        .functions
        .iter()
        .map(|fun| {
            let (mark, style) = if fun.changed { (" +", Style::default().fg(GREEN).bold()) } else { ("  ", Style::default()) };
            let name = match (fun.kind.as_str(), fun.name.split_once('.')) {
                // Older maps carry depth 0 for methods.
                ("method", Some((_, m))) => format!("{}{m}", "  ".repeat(fun.depth.max(1))),
                _ => format!("{}{}", "  ".repeat(fun.depth), fun.name),
            };
            let head = format!("{mark}{:>4} ", fun.start);
            ListItem::new(Line::from(vec![
                Span::styled(mark, Style::default().fg(GREEN)),
                Span::styled(format!("{:>4} ", fun.start), Style::default().fg(FAINT)),
                Span::styled(trunc(&name, w.saturating_sub(head.width())), style),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(if app.focus == Pane::Functions { Style::default().bg(SEL_BG) } else { Style::default() });
    f.render_stateful_widget(list, area, &mut app.fns);
}

fn empty_reason(app: &App) -> &'static str {
    let c = app.card();
    let ts = c.path.as_deref().and_then(lang_for).is_some();
    match c.kind {
        CardKind::Changed if !ts => " TS/JS only.",
        // New maps list every outline function here too, so an outline function means facts.json predates the panel.
        CardKind::Changed if c.outline.iter().any(|o| matches!(o.kind.as_str(), "function" | "class" | "method")) => " re-run /codemapx to list functions.",
        CardKind::Changed => " No functions.",
        CardKind::Context | CardKind::Missing => " Nothing to list.",
    }
}
