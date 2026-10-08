//! Turns a `CellGrid` into the minimal stream of ANSI escape codes needed to update the
//! terminal from what it showed last frame. Pure (no I/O) so it can be unit-tested and
//! used by headless mode to measure bytes/frame.

use std::io::Write;

use super::{rgb_to_ansi256, ColorMode};
use crate::render::cell::{Cell, CellGrid, Rgb};

/// A cell value that never matches a real cell, forcing a redraw.
const DIRTY: Cell = Cell { ch: '\u{0}', fg: Rgb(1, 2, 3), bg: Rgb(3, 2, 1) };

pub struct TermWriter {
    front: Vec<Cell>,
    w: u16,
    h: u16,
    pub mode: ColorMode,
    buf: Vec<u8>,
}

impl TermWriter {
    pub fn new(mode: ColorMode) -> TermWriter {
        TermWriter { front: Vec::new(), w: 0, h: 0, mode, buf: Vec::with_capacity(1 << 16) }
    }

    /// Forget what is on screen; the next frame is a full redraw.
    pub fn invalidate(&mut self) {
        self.front.fill(DIRTY);
    }

    /// Encode the changes needed to show `grid`. Returns an empty slice when nothing changed.
    pub fn encode(&mut self, grid: &CellGrid) -> &[u8] {
        self.buf.clear();
        if grid.w != self.w || grid.h != self.h {
            self.w = grid.w;
            self.h = grid.h;
            self.front = vec![DIRTY; grid.cells.len()];
            // Clear the screen once so stale content from a larger size disappears.
            self.buf.extend_from_slice(b"\x1b[0m\x1b[2J");
        }
        let start_len = self.buf.len();
        self.buf.extend_from_slice(b"\x1b[?2026h");
        let wrapper_len = self.buf.len();

        let w = grid.w as usize;
        let mut pen_fg: Option<Rgb> = None;
        let mut pen_bg: Option<Rgb> = None;
        let mut cursor: Option<(usize, usize)> = None;

        for y in 0..grid.h as usize {
            let row = y * w;
            let mut x = 0;
            while x < w {
                let cell = grid.cells[row + x];
                if cell == self.front[row + x] {
                    x += 1;
                    continue;
                }
                // Move the cursor here: either by re-printing a short same-pen gap or by CUP.
                match cursor {
                    Some((cx, cy)) if cy == y && cx == x => {}
                    Some((cx, cy))
                        if cy == y
                            && x > cx
                            && x - cx <= 4
                            && (cx..x).all(|gx| {
                                let g = grid.cells[row + gx];
                                Some(g.bg) == pen_bg && (g.ch == ' ' || Some(g.fg) == pen_fg)
                            }) =>
                    {
                        for gx in cx..x {
                            let g = grid.cells[row + gx];
                            push_char(&mut self.buf, g.ch);
                            self.front[row + gx] = g;
                        }
                    }
                    _ => {
                        let _ = write!(self.buf, "\x1b[{};{}H", y + 1, x + 1);
                    }
                }
                let need_fg = cell.ch != ' ' && Some(cell.fg) != pen_fg;
                let need_bg = Some(cell.bg) != pen_bg;
                if need_fg || need_bg {
                    self.push_sgr(if need_fg { Some(cell.fg) } else { None }, if need_bg { Some(cell.bg) } else { None });
                    if need_fg {
                        pen_fg = Some(cell.fg);
                    }
                    if need_bg {
                        pen_bg = Some(cell.bg);
                    }
                }
                push_char(&mut self.buf, cell.ch);
                self.front[row + x] = cell;
                // The terminal cursor sits after the char; at the last column (autowrap off)
                // it stays put, so treat its position as unknown.
                cursor = if x + 1 < w { Some((x + 1, y)) } else { None };
                x += 1;
            }
        }

        if self.buf.len() == wrapper_len {
            // Nothing changed: send nothing at all (unless we cleared for a resize).
            self.buf.truncate(start_len);
            return &self.buf;
        }
        self.buf.extend_from_slice(b"\x1b[?2026l");
        &self.buf
    }

    fn push_sgr(&mut self, fg: Option<Rgb>, bg: Option<Rgb>) {
        self.buf.extend_from_slice(b"\x1b[");
        let mut first = true;
        let mut sep = |buf: &mut Vec<u8>| {
            if !first {
                buf.push(b';');
            }
            first = false;
        };
        match self.mode {
            ColorMode::TrueColor => {
                if let Some(c) = fg {
                    sep(&mut self.buf);
                    let _ = write!(self.buf, "38;2;{};{};{}", c.0, c.1, c.2);
                }
                if let Some(c) = bg {
                    sep(&mut self.buf);
                    let _ = write!(self.buf, "48;2;{};{};{}", c.0, c.1, c.2);
                }
            }
            ColorMode::Ansi256 => {
                if let Some(c) = fg {
                    sep(&mut self.buf);
                    let _ = write!(self.buf, "38;5;{}", rgb_to_ansi256(c));
                }
                if let Some(c) = bg {
                    sep(&mut self.buf);
                    let _ = write!(self.buf, "48;5;{}", rgb_to_ansi256(c));
                }
            }
        }
        self.buf.push(b'm');
    }
}

fn push_char(buf: &mut Vec<u8>, ch: char) {
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> CellGrid {
        CellGrid::new(10, 3)
    }

    #[test]
    fn identical_frames_emit_nothing() {
        let mut w = TermWriter::new(ColorMode::TrueColor);
        let g = grid();
        assert!(!w.encode(&g).is_empty());
        assert!(w.encode(&g).is_empty());
    }

    #[test]
    fn single_cell_change_is_small() {
        let mut w = TermWriter::new(ColorMode::TrueColor);
        let mut g = grid();
        w.encode(&g);
        g.put(4, 1, '@', Rgb(255, 0, 0));
        let out = String::from_utf8(w.encode(&g).to_vec()).unwrap();
        assert_eq!(out, "\x1b[?2026h\x1b[2;5H\x1b[38;2;255;0;0;48;2;0;0;0m@\x1b[?2026l");
    }

    #[test]
    fn short_gaps_are_reprinted_not_jumped() {
        let mut w = TermWriter::new(ColorMode::TrueColor);
        let mut g = grid();
        w.encode(&g);
        g.put(1, 0, 'a', Rgb::WHITE);
        g.put(3, 0, 'b', Rgb::WHITE);
        let out = String::from_utf8(w.encode(&g).to_vec()).unwrap();
        // One CUP, one SGR, then "a b" with the space re-printed.
        assert_eq!(out.matches('H').count(), 1);
        assert!(out.contains("a b"));
    }

    #[test]
    fn ansi256_mode_uses_palette_codes() {
        let mut w = TermWriter::new(ColorMode::Ansi256);
        let mut g = grid();
        w.encode(&g);
        g.put(0, 0, 'x', Rgb(255, 0, 0));
        let out = String::from_utf8(w.encode(&g).to_vec()).unwrap();
        assert!(out.contains("38;5;196"));
    }

    #[test]
    fn ansi256_mapping() {
        assert_eq!(rgb_to_ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(rgb_to_ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(rgb_to_ansi256(Rgb(128, 128, 128)), 244);
    }

    #[test]
    fn resize_forces_full_redraw() {
        let mut w = TermWriter::new(ColorMode::TrueColor);
        let g = grid();
        w.encode(&g);
        let g2 = CellGrid::new(12, 3);
        let out = w.encode(&g2).to_vec();
        assert!(out.starts_with(b"\x1b[0m\x1b[2J"));
    }
}
