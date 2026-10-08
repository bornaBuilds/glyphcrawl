//! UI widgets: boxes, bars, wrapped text, buttons.

use super::cell::{Cell, CellGrid, Rgb};

pub const UI_FRAME: Rgb = Rgb(120, 110, 95);
pub const UI_TEXT: Rgb = Rgb(220, 214, 200);
pub const UI_DIM: Rgb = Rgb(130, 126, 118);
pub const UI_GOLD: Rgb = Rgb(240, 200, 110);
pub const UI_BG: Rgb = Rgb(14, 13, 16);

/// A box with a single-line frame, filled with `bg`, optional title in the top edge.
pub fn panel(grid: &mut CellGrid, x: i32, y: i32, w: i32, h: i32, frame: Rgb, bg: Rgb, title: Option<&str>) {
    if w < 2 || h < 2 {
        return;
    }
    for yy in y..y + h {
        for xx in x..x + w {
            let ch = match (xx == x, xx == x + w - 1, yy == y, yy == y + h - 1) {
                (true, _, true, _) => '┌',
                (_, true, true, _) => '┐',
                (true, _, _, true) => '└',
                (_, true, _, true) => '┘',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            grid.set(xx, yy, Cell { ch, fg: frame, bg });
        }
    }
    if let Some(t) = title {
        let t = format!(" {t} ");
        let tx = x + (w - t.chars().count() as i32) / 2;
        grid.text_bg(tx, y, &t, UI_GOLD, bg);
    }
}

/// Double-line frame (for the important things: rewards, death screen).
pub fn panel_double(grid: &mut CellGrid, x: i32, y: i32, w: i32, h: i32, frame: Rgb, bg: Rgb) {
    for yy in y..y + h {
        for xx in x..x + w {
            let ch = match (xx == x, xx == x + w - 1, yy == y, yy == y + h - 1) {
                (true, _, true, _) => '╔',
                (_, true, true, _) => '╗',
                (true, _, _, true) => '╚',
                (_, true, _, true) => '╝',
                (_, _, true, _) | (_, _, _, true) => '═',
                (true, _, _, _) | (_, true, _, _) => '║',
                _ => ' ',
            };
            grid.set(xx, yy, Cell { ch, fg: frame, bg });
        }
    }
}

/// `[#######-----]` bar.
pub fn bar(grid: &mut CellGrid, x: i32, y: i32, w: i32, frac: f32, fill: Rgb, empty: Rgb) {
    let inner = (w - 2).max(1);
    let n = (frac.clamp(0.0, 1.0) * inner as f32).round() as i32;
    grid.put(x, y, '[', UI_DIM);
    for i in 0..inner {
        if i < n {
            grid.put(x + 1 + i, y, '#', fill);
        } else {
            grid.put(x + 1 + i, y, '-', empty);
        }
    }
    grid.put(x + w - 1, y, ']', UI_DIM);
}

/// Word-wrap `text` into lines of at most `width` chars.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Is the mouse inside the rectangle?
pub fn hover(mouse: Option<(i32, i32)>, x: i32, y: i32, w: i32, h: i32) -> bool {
    mouse.is_some_and(|(mx, my)| mx >= x && mx < x + w && my >= y && my < y + h)
}

/// A menu entry; returns true if hovered.
pub fn menu_item(grid: &mut CellGrid, x: i32, y: i32, w: i32, label: &str, selected: bool) {
    if selected {
        let s = format!(" ▸ {label:<width$}", width = (w - 3).max(0) as usize);
        grid.text_bg(x, y, &s, Rgb(20, 16, 10), UI_GOLD);
    } else {
        grid.text(x + 3, y, label, UI_TEXT);
    }
}
