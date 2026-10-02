//! The whole diff shrunk beside it: braille dots where text is, four diff lines per row, changes in color, the visible window shaded.

use ratatui::prelude::*;
use unicode_width::UnicodeWidthChar;

use super::{GREEN, RED, SEL_BG, app::{App, Kind}, fns_pane::MIN_DIFF_WIDTH, pane_block};

/// Columns the panel takes by default, borders included.
pub(super) const WIDTH: u16 = 20;
/// Narrowest a drag can make the panel.
const MIN_WIDTH: u16 = 8;
/// Most source columns one braille dot stands for; longer lines are clipped.
const MAX_SCALE: usize = 4;
const CODE: Color = Color::Rgb(84, 90, 104);
/// Braille dot bits by `[dot row][dot column]`, offsets from U+2800.
const DOTS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// The panel's width out of a `row` columns wide, clamped so the diff keeps `MIN_DIFF_WIDTH`.
pub(super) fn width(want: Option<u16>, row: u16) -> u16 {
    want.unwrap_or(WIDTH).clamp(MIN_WIDTH, row.saturating_sub(MIN_DIFF_WIDTH).max(MIN_WIDTH))
}

/// Diff lines one dot row stands for when `total` lines share `rows` rows.
pub(super) fn per_slot(total: usize, rows: u16) -> usize {
    total.div_ceil(4 * rows.max(1) as usize).max(1)
}

/// What a run of diff lines looks like: its strongest kind and which dot columns hold text.
struct Slot {
    rank: u8,
    color: Color,
    dots: Vec<bool>,
}

fn slot(app: &App, lines: std::ops::Range<usize>, scale: usize, cols: usize) -> Slot {
    let mut s = Slot { rank: 0, color: Color::Reset, dots: vec![false; cols] };
    let end = lines.end.min(app.lines.len());
    for l in &app.lines[lines.start.min(end)..end] {
        let (r, c) = match l.kind {
            Kind::Add => (3, GREEN),
            Kind::Del => (2, RED),
            Kind::Ctx | Kind::Hunk | Kind::Note => (1, CODE),
        };
        // Drop the diff's +/-/space prefix; notes have none.
        let text = if l.kind == Kind::Note { l.text.as_str() } else { l.text.get(1..).unwrap_or("") };
        let mut col = 0;
        let mut any = false;
        for ch in text.chars() {
            let w = if ch == '\t' { 4 } else { ch.width().unwrap_or(0) };
            if !ch.is_whitespace() {
                for d in (col..col + w).map(|c| c / scale).take_while(|&d| d < cols) {
                    (s.dots[d], any) = (true, true);
                }
            }
            col += w;
        }
        if any && r > s.rank {
            (s.rank, s.color) = (r, c);
        }
    }
    s
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
    let widest = app.lines.iter().map(|l| l.text.replace('\t', "    ").chars().filter_map(|c| c.width()).sum::<usize>()).max().unwrap_or(0);
    let scale = widest.div_ceil(2 * w).clamp(1, MAX_SCALE);
    let (top, bottom) = (app.scroll, app.scroll + view.max(1));
    let buf = f.buffer_mut();
    for row in 0..h as usize {
        let first = 4 * row * per;
        if first >= app.lines.len() {
            break;
        }
        let slots: Vec<Slot> = (0..4).map(|d| first + d * per).map(|s| slot(app, s..s + per, scale, 2 * w)).collect();
        let fg = slots.iter().max_by_key(|s| s.rank).map_or(Color::Reset, |s| s.color);
        let bg = if first < bottom && first + 4 * per > top { SEL_BG } else { Color::Reset };
        for x in 0..w {
            let bits: u32 = (0..4).flat_map(|d| (0..2).map(move |c| (d, c))).filter(|&(d, c)| slots[d].dots[2 * x + c]).map(|(d, c)| DOTS[d][c]).sum();
            let sym = if bits == 0 { ' ' } else { char::from_u32(0x2800 + bits).unwrap_or(' ') };
            buf[(inner.x + x as u16, inner.y + row as u16)].set_char(sym).set_style(Style::default().fg(fg).bg(bg));
        }
    }
}

/// The diff line under minimap row `row` of a panel `rows` tall.
pub(super) fn line_at(app: &App, row: u16, rows: u16) -> usize {
    (4 * row as usize * per_slot(app.lines.len(), rows)).min(app.lines.len().saturating_sub(1))
}
