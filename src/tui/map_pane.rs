use ratatui::{prelude::*, widgets::Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{AMBER, BLUE, DIM, FAINT, GREEN, RED, SEL_BG, app::{App, Pane, Rel}, pane_block, trunc, trunc_left};
use crate::map::CardKind;

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(Line::from(" map "), app.focus == Pane::Map);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let n = app.map.columns.len() as u32;
    let cols = Layout::horizontal((0..n).map(|_| Constraint::Ratio(1, n))).spacing(2).split(inner);
    for (c, col) in app.map.columns.iter().enumerate() {
        let w = cols[c].width as usize;
        let mut lines = vec![Line::from(Span::styled(trunc(&col.name.to_uppercase(), w), Style::default().fg(DIM)))];
        lines.extend(col.cards.iter().map(|&i| card_line(app, i, w)));
        f.render_widget(Paragraph::new(lines), cols[c]);
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
