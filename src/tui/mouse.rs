//! Dragging the borders between the map, the middle row, the diff and the panels beside it, wheel-scrolling the pane under the pointer,
//! and clicking the minimap to jump the diff.

use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Position,
};

use super::{
    MIN_ROW_H, fns_pane, minimap,
    app::{App, Divider, Pane},
};

/// Lines one wheel notch moves the diff; other panes move one item.
const WHEEL_LINES: isize = 3;

pub fn handle(app: &mut App, ev: MouseEvent) {
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.drag = grab(app, ev.column, ev.row);
            if app.drag.is_none() {
                jump_minimap(app, ev.column, ev.row);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => match app.drag {
            Some((Divider::FnsDiff, offset)) => resize_fns(app, ev.column as i32 - offset),
            Some((Divider::DiffMinimap, offset)) => resize_minimap(app, ev.column as i32 - offset),
            None => jump_minimap(app, ev.column, ev.row),
            Some((d, offset)) => resize(app, d, ev.row as i32 - offset),
        },
        MouseEventKind::Up(MouseButton::Left) => app.drag = None,
        MouseEventKind::ScrollDown => wheel(app, ev, 1),
        MouseEventKind::ScrollUp => wheel(app, ev, -1),
        _ => {}
    }
}

/// Moves the pane under the pointer without moving focus.
fn wheel(app: &mut App, ev: MouseEvent, d: isize) {
    let [map, _, diff] = app.panes;
    let [from, inside, to] = app.mid_panes;
    let at = Position::new(ev.column, ev.row);
    if app.minimap.contains(at) {
        return app.scroll_by(d * WHEEL_LINES);
    }
    // The functions panel sits inside the diff row, so it is tested first.
    let hit = [(Pane::Map, map), (Pane::From, from), (Pane::Inside, inside), (Pane::To, to), (Pane::Functions, app.fns_pane), (Pane::Diff, diff)]
        .into_iter()
        .find(|(_, r)| r.contains(at));
    match hit {
        Some((Pane::Diff, _)) => app.scroll_by(d * WHEEL_LINES),
        Some((p, _)) => app.move_pane(p, d),
        None => {}
    }
}

/// The divider under (`col`, `row`) — either line of the two borders that meet there — and the pointer's offset from it.
fn grab(app: &App, col: u16, row: u16) -> Option<(Divider, i32)> {
    let [_, mid, diff] = app.panes;
    let rows = [(Divider::MapMid, mid.y), (Divider::MidDiff, diff.y)]
        .into_iter()
        .filter(|_| !app.diff_full)
        .find(|&(_, y)| y > 0 && (row == y || row + 1 == y))
        .map(|(d, y)| (d, row as i32 - y as i32));
    // Only the panel's side rows, so the diff's top border stays a row divider.
    let p = app.fns_pane;
    let x = p.right();
    let fns = (p.width > 0 && row > p.y && row + 1 < p.bottom() && (col == x || col + 1 == x)).then(|| (Divider::FnsDiff, col as i32 - x as i32));
    let m = app.minimap;
    let mm = (m.width > 0 && row > m.y && row + 1 < m.bottom() && (col == m.x || col + 1 == m.x)).then(|| (Divider::DiffMinimap, col as i32 - m.x as i32));
    rows.or(fns).or(mm)
}

/// Scrolls the diff so the line under a press or drag inside the minimap sits mid-pane.
fn jump_minimap(app: &mut App, col: u16, row: u16) {
    let m = app.minimap;
    let inner = m.y + 1..m.bottom().saturating_sub(1);
    if m.width == 0 || col <= m.x || col + 1 >= m.right() || !inner.contains(&row) {
        return;
    }
    let rows = inner.end - inner.start;
    let half = app.panes[2].height.saturating_sub(2) as usize / 2;
    app.scroll = minimap::line_at(app, row - inner.start, rows).saturating_sub(half);
}

/// Moves the minimap's left edge to `x`, clamped like drawing clamps it.
fn resize_minimap(app: &mut App, x: i32) {
    let row = app.panes[2];
    app.minimap_width = Some(minimap::width(Some((row.right() as i32 - x).max(0) as u16), row.width));
}

/// Moves the functions panel's right edge to `x`, clamped like drawing clamps it.
fn resize_fns(app: &mut App, x: i32) {
    let row = app.panes[2];
    app.fns_width = Some(fns_pane::width(Some((x - app.fns_pane.x as i32).max(0) as u16), row.width - app.minimap.width));
}

/// Moves divider `d` to `y`, trading rows only between the two panes it separates.
fn resize(app: &mut App, d: Divider, y: i32) {
    let [map, mid, diff] = app.panes;
    let (above, below) = match d {
        Divider::MapMid => (map, mid),
        Divider::MidDiff => (mid, diff),
        Divider::FnsDiff | Divider::DiffMinimap => return,
    };
    let total = above.height + below.height;
    let h = (y - above.y as i32).clamp(MIN_ROW_H as i32, total.saturating_sub(MIN_ROW_H).max(MIN_ROW_H) as i32) as u16;
    app.heights = Some(if d == Divider::MapMid { (h, total.saturating_sub(h)) } else { (map.height, h) });
}
