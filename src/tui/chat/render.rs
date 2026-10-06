//! A vt100 screen drawn into a ratatui buffer, from wsx.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

/// Cells past the screen's size are blanked.
pub fn render_screen(screen: &vt100::Screen, buf: &mut Buffer, area: Rect) {
    let (rows, cols) = screen.size();
    for y in 0..area.height {
        for x in 0..area.width {
            let out = &mut buf[(area.x + x, area.y + y)];
            let Some(cell) = (y < rows && x < cols).then(|| screen.cell(y, x)).flatten() else {
                out.reset();
                continue;
            };
            // has_contents() is false only for blank cells; a wide glyph's continuation cell says true but is empty.
            let glyph = if cell.has_contents() { cell.contents() } else { String::new() };
            out.set_symbol(if glyph.is_empty() { " " } else { &glyph });
            out.set_style(style(cell));
        }
    }
}

fn style(cell: &vt100::Cell) -> Style {
    let mut s = Style::default().fg(color(cell.fgcolor())).bg(color(cell.bgcolor()));
    for (on, m) in [(cell.bold(), Modifier::BOLD), (cell.italic(), Modifier::ITALIC), (cell.underline(), Modifier::UNDERLINED), (cell.inverse(), Modifier::REVERSED)] {
        if on {
            s = s.add_modifier(m);
        }
    }
    s
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
