//! The whole diff shrunk beside it: two diff lines per row as half blocks, changes in color, the visible window shaded.

use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

use super::{GREEN, RED, SEL_BG, app::{App, Kind}, fns_pane::MIN_DIFF_WIDTH, pane_block};

/// Columns the panel takes by default, borders included.
pub(super) const WIDTH: u16 = 20;
/// Narrowest a drag can make the panel.
const MIN_WIDTH: u16 = 8;
/// Most source columns one minimap cell stands for; longer lines are clipped.
const MAX_SCALE: usize = 4;
const CODE: Color = Color::Rgb(84, 90, 104);

/// The panel's width out of a `row` columns wide, clamped so the diff keeps `MIN_DIFF_WIDTH`.
pub(super) fn width(want: Option<u16>, row: u16) -> u16 {
    want.unwrap_or(WIDTH).clamp(MIN_WIDTH, row.saturating_sub(MIN_DIFF_WIDTH).max(MIN_WIDTH))
}

/// Diff lines one half-row stands for when `total` lines share `rows` rows.
pub(super) fn per_slot(total: usize, rows: u16) -> usize {
    total.div_ceil(2 * rows.max(1) as usize).max(1)
}

/// What a run of diff lines looks like: its strongest kind and the columns its text spans.
#[derive(Clone, Copy)]
struct Slot {
    color: Option<Color>,
    from: usize,
    to: usize,
}

fn slot(app: &App, lines: std::ops::Range<usize>) -> Slot {
    let mut s = Slot { color: None, from: usize::MAX, to: 0 };
    let mut rank = 0;
    let end = lines.end.min(app.lines.len());
    for l in &app.lines[lines.start.min(end)..end] {
        let (r, c) = match l.kind {
            Kind::Add => (3, GREEN),
            Kind::Del => (2, RED),
            Kind::Ctx | Kind::Hunk | Kind::Note => (1, CODE),
        };
        // Drop the diff's +/-/space prefix; notes have none.
        let text = if l.kind == Kind::Note { l.text.as_str() } else { l.text.get(1..).unwrap_or("") };
        let text = text.replace('\t', "    ");
        let body = text.trim_start();
        if body.trim_end().is_empty() {
            continue;
        }
        let from = text.width() - body.width();
        s.from = s.from.min(from);
        s.to = s.to.max(from + body.trim_end().width());
        if r > rank {
            (rank, s.color) = (r, Some(c));
        }
    }
    s
}

impl Slot {
    fn covers(&self, x: usize, scale: usize) -> Option<Color> {
        self.color.filter(|_| x * scale < self.to && (x + 1) * scale > self.from)
    }
}

/// `view` is how many diff lines the diff pane shows, so the window it covers can be shaded.
pub(super) fn draw(f: &mut Frame, app: &App, area: Rect, view: usize) {
    let block = pane_block(Line::from(" minimap "), false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let (w, h) = (inner.width as usize, inner.height);
    if w == 0 || h == 0 || app.lines.is_empty() {
        return;
    }
    let per = per_slot(app.lines.len(), h);
    let widest = app.lines.iter().map(|l| l.text.replace('\t', "    ").width()).max().unwrap_or(0);
    let scale = widest.div_ceil(w).clamp(1, MAX_SCALE);
    let (top, bottom) = (app.scroll, app.scroll + view.max(1));
    let buf = f.buffer_mut();
    for row in 0..h as usize {
        let first = 2 * row * per;
        if first >= app.lines.len() {
            break;
        }
        let (a, b) = (slot(app, first..first + per), slot(app, first + per..first + 2 * per));
        let bg = if first < bottom && first + 2 * per > top { SEL_BG } else { Color::Reset };
        for x in 0..w {
            let (sym, fg, cell_bg) = match (a.covers(x, scale), b.covers(x, scale)) {
                (Some(t), Some(u)) => ("▀", t, u),
                (Some(t), None) => ("▀", t, bg),
                (None, Some(u)) => ("▄", u, bg),
                (None, None) => (" ", Color::Reset, bg),
            };
            buf[(inner.x + x as u16, inner.y + row as u16)].set_symbol(sym).set_style(Style::default().fg(fg).bg(cell_bg));
        }
    }
}

/// The diff line under minimap row `row` of a panel `rows` tall.
pub(super) fn line_at(app: &App, row: u16, rows: u16) -> usize {
    (2 * row as usize * per_slot(app.lines.len(), rows)).min(app.lines.len().saturating_sub(1))
}
