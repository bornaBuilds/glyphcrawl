//! The one contract between the game and every backend: a grid of characters with colours.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    pub fn scale(self, k: f32) -> Rgb {
        let f = |c: u8| (c as f32 * k).round().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0), f(self.1), f(self.2))
    }

    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }

    pub fn to_f32(self) -> [f32; 3] {
        [self.0 as f32 / 255.0, self.1 as f32 / 255.0, self.2 as f32 / 255.0]
    }

    pub fn from_f32(c: [f32; 3]) -> Rgb {
        let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb(f(c[0]), f(c[1]), f(c[2]))
    }

    pub fn luma(self) -> f32 {
        (0.2126 * self.0 as f32 + 0.7152 * self.1 as f32 + 0.0722 * self.2 as f32) / 255.0
    }

    /// Parse "#rrggbb".
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let s = s.strip_prefix('#')?;
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Cell {
    pub const BLANK: Cell = Cell { ch: ' ', fg: Rgb::WHITE, bg: Rgb::BLACK };

    pub fn new(ch: char, fg: Rgb) -> Cell {
        Cell { ch, fg, bg: Rgb::BLACK }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Cell::BLANK
    }
}

#[derive(Clone, Debug)]
pub struct CellGrid {
    pub w: u16,
    pub h: u16,
    pub cells: Vec<Cell>,
}

impl CellGrid {
    pub fn new(w: u16, h: u16) -> CellGrid {
        CellGrid { w, h, cells: vec![Cell::BLANK; w as usize * h as usize] }
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        if w != self.w || h != self.h {
            *self = CellGrid::new(w, h);
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::BLANK);
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w as i32 && y < self.h as i32
    }

    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        y as usize * self.w as usize + x as usize
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        if self.in_bounds(x, y) { Some(&self.cells[self.idx(x, y)]) } else { None }
    }

    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if self.in_bounds(x, y) {
            let i = self.idx(x, y);
            Some(&mut self.cells[i])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        if let Some(c) = self.get_mut(x, y) {
            *c = cell;
        }
    }

    /// Set glyph + fg, keep the background.
    pub fn put(&mut self, x: i32, y: i32, ch: char, fg: Rgb) {
        if let Some(c) = self.get_mut(x, y) {
            c.ch = ch;
            c.fg = fg;
        }
    }

    /// Write a string; returns the column after the last char.
    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgb) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            self.put(cx, y, ch, fg);
            cx += 1;
        }
        cx
    }

    pub fn text_bg(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Rgb) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            self.set(cx, y, Cell { ch, fg, bg });
            cx += 1;
        }
        cx
    }

    pub fn text_centered(&mut self, y: i32, s: &str, fg: Rgb) {
        let x = (self.w as i32 - s.chars().count() as i32) / 2;
        self.text(x, y, s, fg);
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, cell: Cell) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, cell);
            }
        }
    }

    /// Dim every cell in a rectangle (used behind modals).
    pub fn dim(&mut self, k: f32) {
        for c in &mut self.cells {
            c.fg = c.fg.scale(k);
            c.bg = c.bg.scale(k);
        }
    }

    /// Plain-text dump (used by headless mode and snapshot tests).
    pub fn to_text(&self) -> String {
        let mut s = String::with_capacity((self.w as usize + 1) * self.h as usize);
        for y in 0..self.h as i32 {
            let line: String = (0..self.w as i32).map(|x| self.cells[self.idx(x, y)].ch).collect();
            s.push_str(line.trim_end());
            s.push('\n');
        }
        s
    }
}
