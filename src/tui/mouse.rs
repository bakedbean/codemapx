//! Dragging the borders between the map, the middle row and the diff, and wheel-scrolling the pane under the pointer.

use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Position,
};

use super::{
    MIN_ROW_H,
    app::{App, Divider, Pane},
};

/// Lines one wheel notch moves the diff; other panes move one item.
const WHEEL_LINES: isize = 3;

pub fn handle(app: &mut App, ev: MouseEvent) {
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => app.drag = grab(app, ev.row),
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((d, offset)) = app.drag {
                resize(app, d, ev.row as i32 - offset);
            }
        }
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

/// The divider under `row` — either line of the two borders that meet there — and the row's offset from it.
fn grab(app: &App, row: u16) -> Option<(Divider, i32)> {
    if app.diff_full {
        return None;
    }
    let [_, mid, diff] = app.panes;
    [(Divider::MapMid, mid.y), (Divider::MidDiff, diff.y)]
        .into_iter()
        .find(|&(_, y)| y > 0 && (row == y || row + 1 == y))
        .map(|(d, y)| (d, row as i32 - y as i32))
}

/// Moves divider `d` to `y`, trading rows only between the two panes it separates.
fn resize(app: &mut App, d: Divider, y: i32) {
    let [map, mid, diff] = app.panes;
    let (above, below) = match d {
        Divider::MapMid => (map, mid),
        Divider::MidDiff => (mid, diff),
    };
    let total = above.height + below.height;
    let h = (y - above.y as i32).clamp(MIN_ROW_H as i32, total.saturating_sub(MIN_ROW_H).max(MIN_ROW_H) as i32) as u16;
    app.heights = Some(match d {
        Divider::MapMid => (h, total.saturating_sub(h)),
        Divider::MidDiff => (map.height, h),
    });
}
