use ratatui::{
    prelude::*,
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{AMBER, BLUE, DIM, SEL_BG, app::{App, Pane}, pane_block, trunc, wrap};
use crate::map::CardKind;

fn link_items(app: &App, links: &[(usize, usize)], color: Color, width: usize) -> Vec<ListItem<'static>> {
    if links.is_empty() {
        return vec![ListItem::new(Span::styled("Nothing in this branch.", Style::default().fg(DIM)))];
    }
    links
        .iter()
        .map(|&(i, k)| {
            let card = &app.map.cards[i];
            let link = &app.map.links[k];
            let ghost = card.kind != CardKind::Changed;
            let mut name = Span::styled(card.name.clone(), Style::default().fg(color).bold());
            if ghost {
                name = name.italic();
            }
            let mut lines = vec![Line::from(vec![name, Span::styled(if ghost { " ┆" } else { "" }, Style::default().fg(DIM))])];
            lines.extend(wrap(&link.reason, width.saturating_sub(2)).into_iter().map(|l| Line::from(Span::styled(format!("  {l}"), Style::default().fg(DIM)))));
            ListItem::new(lines)
        })
        .collect()
}

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)]).split(area);

    for (k, incoming) in [(0usize, true), (2usize, false)] {
        let pane = if incoming { Pane::From } else { Pane::To };
        let color = if incoming { AMBER } else { BLUE };
        let title = Line::from(Span::styled(if incoming { " came from " } else { " leads to " }, Style::default().fg(color)));
        let block = pane_block(title, app.focus == pane);
        let w = block.inner(cols[k]).width as usize;
        let links = app.links(incoming);
        let list = List::new(link_items(app, &links, color, w))
            .block(block)
            .highlight_style(if app.focus == pane { Style::default().bg(SEL_BG) } else { Style::default() });
        let st = if incoming { &mut app.from } else { &mut app.to };
        f.render_stateful_widget(list, cols[k], st);
    }

    let card = app.card().clone();
    let title = Line::from(Span::styled(format!(" {} ", card.name), Style::default().bold()));
    let block = pane_block(title, app.focus == Pane::Inside);
    let inner = block.inner(cols[1]);
    f.render_widget(block, cols[1]);
    let w = inner.width as usize;
    let path = match card.kind {
        CardKind::Missing => "not built".to_string(),
        _ => card.path.clone().unwrap_or_default(),
    };
    let mut head: Vec<Line> = vec![Line::from(Span::styled(trunc(&path, w), Style::default().fg(DIM)))];
    let what = wrap(&card.what, w);
    let budget = if card.outline.is_empty() { inner.height as usize } else { 4 };
    let clipped = what.len() > budget;
    head.extend(what.into_iter().take(budget).map(Line::from));
    if clipped {
        if let Some(last) = head.last_mut() {
            last.spans.push(Span::styled(" …", Style::default().fg(DIM)));
        }
    }
    let parts = Layout::vertical([Constraint::Length(head.len() as u16), Constraint::Length(1), Constraint::Min(0)]).split(inner);
    f.render_widget(Paragraph::new(head), parts[0]);
    if !card.outline.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("INSIDE THIS FILE", Style::default().fg(DIM))), parts[1]);
        let items: Vec<ListItem> = card
            .outline
            .iter()
            .map(|o| {
                let head = format!("{:>4} {}", o.start, o.name);
                let rest = w.saturating_sub(head.width() + 3);
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:>4} ", o.start), Style::default().fg(DIM)),
                    Span::styled(o.name.clone(), Style::default().bold()),
                    Span::styled(format!("  {}", trunc(&o.note, rest)), Style::default().fg(DIM)),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(if app.focus == Pane::Inside { Style::default().bg(SEL_BG) } else { Style::default() });
        f.render_stateful_widget(list, parts[2], &mut app.inside);
    }
}
