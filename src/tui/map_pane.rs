use ratatui::{prelude::*, widgets::Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{AMBER, BLUE, DIM, FAINT, GREEN, RED, SEL_BG, app::{App, Pane, Rel}, pane_block, trunc, trunc_left};
use crate::map::CardKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnView {
    Expanded,
    Collapsed,
}

pub const FULL_WIDTH: u16 = 170;
pub const MIN_WIDTH: u16 = 100;
const COLLAPSED_W: u16 = 16;

/// Columns `t` swaps with the rest when the terminal is too narrow for everything.
pub fn hideable(name: &str) -> bool {
    name.eq_ignore_ascii_case("tests") || name.eq_ignore_ascii_case("docs")
}

/// How each column renders at `width`; None when the map can't fit at all.
pub fn column_layout(names: &[&str], width: u16, show_hidden: bool) -> Option<Vec<ColumnView>> {
    if width < MIN_WIDTH {
        return None;
    }
    let hideable: Vec<bool> = names.iter().map(|n| hideable(n)).collect();
    if width >= FULL_WIDTH || !hideable.contains(&true) {
        return Some(vec![ColumnView::Expanded; names.len()]);
    }
    Some(hideable.iter().map(|&h| if h != show_hidden { ColumnView::Collapsed } else { ColumnView::Expanded }).collect())
}

fn collapsed_lines(app: &App, col: &crate::map::MapColumn, w: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(trunc(&format!("{} · {}", col.name.to_uppercase(), col.cards.len()), w), Style::default().fg(DIM)))];
    if col.cards.contains(&app.cur) {
        lines.push(Line::from(Span::styled("▶ selected", Style::default().fg(Color::White).bold())));
    }
    let count = |want: fn(&Rel) -> bool| col.cards.iter().filter(|&&i| want(&app.rel(i))).count();
    let from = count(|r| matches!(r, Rel::From));
    if from > 0 {
        lines.push(Line::from(Span::styled(format!("◂ {from} came from"), Style::default().fg(AMBER))));
    }
    let to = count(|r| matches!(r, Rel::To));
    if to > 0 {
        lines.push(Line::from(Span::styled(format!("▸ {to} leads to"), Style::default().fg(BLUE))));
    }
    lines
}

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(Line::from(" map "), app.focus == Pane::Map);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let views = app.column_views(area.width);
    let constraints = views.iter().map(|v| match v {
        ColumnView::Expanded => Constraint::Fill(1),
        ColumnView::Collapsed => Constraint::Length(COLLAPSED_W),
    });
    let cols = Layout::horizontal(constraints).spacing(2).split(inner);
    for (c, col) in app.map.columns.iter().enumerate() {
        let w = cols[c].width as usize;
        let lines = match views[c] {
            ColumnView::Collapsed => collapsed_lines(app, col, w),
            ColumnView::Expanded => {
                let mut l = vec![Line::from(Span::styled(trunc(&col.name.to_uppercase(), w), Style::default().fg(DIM)))];
                let sel = col.cards.iter().position(|&i| i == app.cur);
                let (start, end) = window(col.cards.len(), (cols[c].height as usize).saturating_sub(1), sel);
                let hint = |t: String| Line::from(Span::styled(t, Style::default().fg(DIM)));
                if start > 0 {
                    l.push(hint(format!("↑ {start} more")));
                }
                l.extend(col.cards[start..end].iter().map(|&i| card_line(app, i, w)));
                if end < col.cards.len() {
                    l.push(hint(format!("↓ {} more", col.cards.len() - end)));
                }
                l
            }
        };
        f.render_widget(Paragraph::new(lines), cols[c]);
    }
}

/// Cards `start..end` of a column that fit in `rows` along with any ↑/↓ hint rows, keeping `sel` in view.
fn window(len: usize, rows: usize, sel: Option<usize>) -> (usize, usize) {
    if len <= rows {
        return (0, len);
    }
    let p = sel.unwrap_or(0);
    if p + 1 < rows {
        (0, rows.saturating_sub(1))
    } else if p >= len - rows.saturating_sub(1) {
        (len - rows.saturating_sub(1), len)
    } else {
        let shown = rows.saturating_sub(2).max(1);
        (p + 1 - shown, p + 1)
    }
}

pub(super) fn card_line(app: &App, i: usize, w: usize) -> Line<'static> {
    let card = &app.map.cards[i];
    let rel = app.rel(i);
    let (marker, color) = match rel {
        Rel::Sel => ("▶ ", Color::White),
        Rel::From => ("◂ ", AMBER),
        Rel::To => ("▸ ", BLUE),
        Rel::None => ("  ", FAINT),
    };
    let ghost = card.kind != CardKind::Changed;
    let stats = match card.kind {
        CardKind::Context => "┆".to_string(),
        CardKind::Missing => "not built".to_string(),
        CardKind::Changed if card.del > 0 => format!("+{} −{}", card.add, card.del),
        CardKind::Changed => format!("+{}", card.add),
    };
    let name = trunc_left(&card.name, w.saturating_sub(2 + stats.width() + 1));
    let pad = w.saturating_sub(2 + name.width() + stats.width());
    let mut name_style = Style::default().fg(color);
    if ghost {
        name_style = name_style.italic();
    }
    let mut row_style = Style::default();
    if i == app.cur {
        name_style = name_style.bold();
        row_style = row_style.bg(SEL_BG);
    }
    let unrelated = matches!(rel, Rel::None);
    let stat_style = |c: Color| Style::default().fg(if unrelated { FAINT } else { c });
    let mut spans = vec![Span::styled(marker, Style::default().fg(color)), Span::styled(name, name_style), Span::raw(" ".repeat(pad))];
    if ghost {
        spans.push(Span::styled(stats, stat_style(DIM)));
    } else if let Some((a, d)) = stats.split_once(' ') {
        spans.push(Span::styled(a.to_string(), stat_style(GREEN)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(d.to_string(), stat_style(RED)));
    } else {
        spans.push(Span::styled(stats, stat_style(GREEN)));
    }
    Line::from(spans).style(row_style)
}
