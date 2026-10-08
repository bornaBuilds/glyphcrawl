//! Draws `Cell`s into a 0RGB pixel buffer with fontdue. Shared by the window backend and the
//! headless `--png` screenshot. Cells are forced to be exactly twice as tall as wide.

use std::collections::HashMap;

use crate::render::cell::{Cell, CellGrid, Rgb};

struct Glyph {
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    cov: Vec<u8>,
}

pub struct GlyphRaster {
    font: fontdue::Font,
    pub px: f32,
    pub cell_w: usize,
    pub cell_h: usize,
    baseline: i32,
    glyphs: HashMap<char, Glyph>,
}

impl GlyphRaster {
    pub fn new(font_bytes: &[u8], px: f32) -> anyhow::Result<GlyphRaster> {
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
            .map_err(|e| anyhow::anyhow!("font: {e}"))?;
        let mut g = GlyphRaster { font, px, cell_w: 8, cell_h: 16, baseline: 12, glyphs: HashMap::new() };
        g.set_px(px);
        Ok(g)
    }

    pub fn set_px(&mut self, px: f32) {
        self.px = px;
        let m = self.font.metrics('M', px);
        self.cell_w = (m.advance_width.round() as usize).max(3);
        self.cell_h = self.cell_w * 2;
        let (ascent, descent) = match self.font.horizontal_line_metrics(px) {
            Some(l) => (l.ascent, l.descent),
            None => (px * 0.8, -px * 0.2),
        };
        let line_h = ascent - descent;
        self.baseline = ((self.cell_h as f32 - line_h) / 2.0 + ascent).round() as i32;
        self.glyphs.clear();
    }

    fn glyph(&mut self, ch: char) -> &Glyph {
        let (font, px, cw, base) = (&self.font, self.px, self.cell_w, self.baseline);
        self.glyphs.entry(ch).or_insert_with(|| {
            let (m, cov) = font.rasterize(ch, px);
            let x = ((cw as f32 - m.advance_width) / 2.0).round() as i32 + m.xmin;
            let y = base - (m.ymin + m.height as i32);
            Glyph { w: m.width, h: m.height, x, y, cov }
        })
    }

    /// Draw one cell at cell coords (cx, cy) into `buf` (width `bw`, height `bh` pixels).
    pub fn draw_cell(&mut self, buf: &mut [u32], bw: usize, bh: usize, cx: usize, cy: usize, cell: Cell) {
        let (cw, chh) = (self.cell_w, self.cell_h);
        let (x0, y0) = (cx * cw, cy * chh);
        let bg = pack(cell.bg);
        for y in y0..(y0 + chh).min(bh) {
            let row = y * bw;
            for x in x0..(x0 + cw).min(bw) {
                buf[row + x] = bg;
            }
        }
        if cell.ch == ' ' {
            return;
        }
        if let Some(segs) = box_segments(cell.ch) {
            draw_box(buf, bw, bh, x0, y0, cw, chh, segs, cell.fg);
            return;
        }
        let g = self.glyph(cell.ch);
        for j in 0..g.h {
            let py = y0 as i32 + g.y + j as i32;
            if py < y0 as i32 || py >= (y0 + chh) as i32 || py as usize >= bh {
                continue;
            }
            for i in 0..g.w {
                let px = x0 as i32 + g.x + i as i32;
                if px < 0 || px as usize >= bw {
                    continue;
                }
                let a = g.cov[j * g.w + i];
                if a == 0 {
                    continue;
                }
                buf[py as usize * bw + px as usize] = pack(cell.bg.lerp(cell.fg, a as f32 / 255.0));
            }
        }
    }

    /// Rasterize a whole grid; returns (width, height, pixels).
    pub fn render_grid(&mut self, grid: &CellGrid) -> (usize, usize, Vec<u32>) {
        let (bw, bh) = (grid.w as usize * self.cell_w, grid.h as usize * self.cell_h);
        let mut buf = vec![0u32; bw * bh];
        for y in 0..grid.h as usize {
            for x in 0..grid.w as usize {
                self.draw_cell(&mut buf, bw, bh, x, y, grid.cells[y * grid.w as usize + x]);
            }
        }
        (bw, bh, buf)
    }
}

pub fn pack(c: Rgb) -> u32 {
    ((c.0 as u32) << 16) | ((c.1 as u32) << 8) | c.2 as u32
}

/// (up, down, left, right) stroke weights: 0 none, 1 light, 2 double.
fn box_segments(ch: char) -> Option<[u8; 4]> {
    Some(match ch {
        '─' => [0, 0, 1, 1],
        '│' => [1, 1, 0, 0],
        '┌' => [0, 1, 0, 1],
        '┐' => [0, 1, 1, 0],
        '└' => [1, 0, 0, 1],
        '┘' => [1, 0, 1, 0],
        '├' => [1, 1, 0, 1],
        '┤' => [1, 1, 1, 0],
        '┬' => [0, 1, 1, 1],
        '┴' => [1, 0, 1, 1],
        '┼' => [1, 1, 1, 1],
        '═' => [0, 0, 2, 2],
        '║' => [2, 2, 0, 0],
        '╔' => [0, 2, 0, 2],
        '╗' => [0, 2, 2, 0],
        '╚' => [2, 0, 0, 2],
        '╝' => [2, 0, 2, 0],
        '╠' => [2, 2, 0, 2],
        '╣' => [2, 2, 2, 0],
        '╦' => [0, 2, 2, 2],
        '╩' => [2, 0, 2, 2],
        '╬' => [2, 2, 2, 2],
        _ => return None,
    })
}

/// Box-drawing characters are drawn as lines so they join seamlessly between cells.
#[allow(clippy::too_many_arguments)]
fn draw_box(buf: &mut [u32], bw: usize, bh: usize, x0: usize, y0: usize, cw: usize, chh: usize, segs: [u8; 4], fg: Rgb) {
    let (cw, chh) = (cw as i32, chh as i32);
    let (mx, my) = (x0 as i32 + cw / 2, y0 as i32 + chh / 2);
    let t = (cw / 8).max(1);
    let gap = (cw / 4).max(2);
    let col = pack(fg);
    let mut rect = |x: i32, y: i32, w: i32, h: i32| {
        for yy in y.max(0)..(y + h).min(bh as i32) {
            for xx in x.max(0)..(x + w).min(bw as i32) {
                buf[yy as usize * bw + xx as usize] = col;
            }
        }
    };
    let [up, down, left, right] = segs;
    let offsets = |w: u8| -> Vec<i32> { if w == 2 { vec![-gap / 2, gap / 2] } else { vec![0] } };
    for (weight, from, to) in [(up, y0 as i32, my + t), (down, my, y0 as i32 + chh)] {
        if weight > 0 {
            for o in offsets(weight) {
                rect(mx + o - t / 2, from, t, to - from);
            }
        }
    }
    for (weight, from, to) in [(left, x0 as i32, mx + t), (right, mx, x0 as i32 + cw)] {
        if weight > 0 {
            for o in offsets(weight) {
                rect(from, my + o - t / 2, to - from, t);
            }
        }
    }
}

/// Minimal PNG writer (stored deflate blocks, no compression crate needed).
pub fn write_png(path: &std::path::Path, w: usize, h: usize, px: &[u32]) -> std::io::Result<()> {
    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let p = px[y * w + x];
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + v as u32) % 65521;
        b = (b + a) % 65521;
    }
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push(if i + 1 == chunks.len() { 1 } else { 0 });
        let len = c.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}
