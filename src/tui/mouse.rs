//! Dragging the borders between the map, the middle row and the diff.

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::{
    MIN_ROW_H,
    app::{App, Divider},
};

pub fn handle(app: &mut App, ev: MouseEvent) {
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => app.drag = grab(app, ev.row),
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((d, offset)) = app.drag {
                resize(app, d, ev.row as i32 - offset);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => app.drag = None,
        _ => {}
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
