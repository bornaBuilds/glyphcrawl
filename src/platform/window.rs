//! Native window backend: minifb for the window + input, fontdue to rasterize glyphs.
//! Cells are forced to be exactly twice as tall as they are wide so the isometric
//! projection matches the terminal. The mouse wheel changes the font size ("zoom").

use std::collections::HashMap;
use std::time::Instant;

use minifb::{Key as MKey, KeyRepeat, MouseButton as MButton, MouseMode, ScaleMode, Window, WindowOptions};

use super::{Backend, Caps, ColorMode, InputState, Key, MouseButton, PresentStats};
use crate::render::cell::{Cell, CellGrid, Rgb};

const MIN_PX: f32 = 8.0;
const MAX_PX: f32 = 40.0;

struct Glyph {
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    cov: Vec<u8>,
}

pub struct WindowBackend {
    window: Window,
    font: fontdue::Font,
    px: f32,
    cell_w: usize,
    cell_h: usize,
    baseline: i32,
    glyphs: HashMap<char, Glyph>,
    buf: Vec<u32>,
    win_w: usize,
    win_h: usize,
    cols: u16,
    rows: u16,
    front: Vec<Cell>,
    held_prev: Vec<Key>,
    btn_prev: [bool; 3],
}

impl WindowBackend {
    pub fn new(font_bytes: &[u8], px: f32) -> anyhow::Result<WindowBackend> {
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
            .map_err(|e| anyhow::anyhow!("font: {e}"))?;
        let (win_w, win_h) = (1600, 900);
        let mut window = Window::new(
            "glyphcrawl",
            win_w,
            win_h,
            WindowOptions { resize: true, scale_mode: ScaleMode::UpperLeft, ..WindowOptions::default() },
        )?;
        window.set_target_fps(0);
        let mut wb = WindowBackend {
            window,
            font,
            px,
            cell_w: 8,
            cell_h: 16,
            baseline: 12,
            glyphs: HashMap::new(),
            buf: Vec::new(),
            win_w,
            win_h,
            cols: 0,
            rows: 0,
            front: Vec::new(),
            held_prev: Vec::new(),
            btn_prev: [false; 3],
        };
        wb.set_font_px(px);
        Ok(wb)
    }

    fn set_font_px(&mut self, px: f32) {
        self.px = px.clamp(MIN_PX, MAX_PX);
        let m = self.font.metrics('M', self.px);
        self.cell_w = (m.advance_width.round() as usize).max(4);
        self.cell_h = self.cell_w * 2;
        let (ascent, descent) = match self.font.horizontal_line_metrics(self.px) {
            Some(l) => (l.ascent, l.descent),
            None => (self.px * 0.8, -self.px * 0.2),
        };
        let line_h = ascent - descent;
        self.baseline = ((self.cell_h as f32 - line_h) / 2.0 + ascent).round() as i32;
        self.glyphs.clear();
        self.relayout();
    }

    fn relayout(&mut self) {
        self.buf = vec![0; self.win_w * self.win_h];
        self.cols = (self.win_w / self.cell_w).max(1) as u16;
        self.rows = (self.win_h / self.cell_h).max(1) as u16;
        self.front.clear();
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

    fn draw_cell(&mut self, cx: usize, cy: usize, cell: Cell) {
        let (cw, ch_) = (self.cell_w, self.cell_h);
        let (x0, y0) = (cx * cw, cy * ch_);
        let bg = pack(cell.bg);
        for y in y0..(y0 + ch_).min(self.win_h) {
            let row = y * self.win_w;
            for x in x0..(x0 + cw).min(self.win_w) {
                self.buf[row + x] = bg;
            }
        }
        if cell.ch == ' ' {
            return;
        }
        if let Some(segs) = box_segments(cell.ch) {
            self.draw_box(x0, y0, segs, cell.fg);
            return;
        }
        let (win_w, win_h) = (self.win_w, self.win_h);
        let g = self.glyph(cell.ch);
        let (gw, gh, gx, gy) = (g.w, g.h, g.x, g.y);
        let cov = g.cov.clone();
        for j in 0..gh {
            let py = y0 as i32 + gy + j as i32;
            if py < y0 as i32 || py >= (y0 + ch_) as i32 || py as usize >= win_h {
                continue;
            }
            for i in 0..gw {
                let px = x0 as i32 + gx + i as i32;
                if px < 0 || px as usize >= win_w {
                    continue;
                }
                let a = cov[j * gw + i];
                if a == 0 {
                    continue;
                }
                let idx = py as usize * win_w + px as usize;
                self.buf[idx] = pack(cell.bg.lerp(cell.fg, a as f32 / 255.0));
            }
        }
    }

    /// Box-drawing characters are drawn as lines so they join seamlessly between cells.
    fn draw_box(&mut self, x0: usize, y0: usize, segs: [u8; 4], fg: Rgb) {
        let (cw, chh) = (self.cell_w as i32, self.cell_h as i32);
        let (mx, my) = (x0 as i32 + cw / 2, y0 as i32 + chh / 2);
        let t = (cw / 8).max(1);
        let gap = (cw / 4).max(2);
        let col = pack(fg);
        let mut rect = |x: i32, y: i32, w: i32, h: i32| {
            for yy in y.max(0)..(y + h).min(self.win_h as i32) {
                for xx in x.max(0)..(x + w).min(self.win_w as i32) {
                    self.buf[yy as usize * self.win_w + xx as usize] = col;
                }
            }
        };
        let [up, down, left, right] = segs;
        let offsets = |w: u8| -> Vec<i32> { if w == 2 { vec![-gap / 2, gap / 2] } else { vec![0] } };
        // Vertical strokes.
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
}

fn pack(c: Rgb) -> u32 {
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

fn map_key(k: MKey) -> Option<Key> {
    use MKey::*;
    let c = |c: char| Some(Key::Char(c));
    match k {
        A => c('a'), B => c('b'), C => c('c'), D => c('d'), E => c('e'), F => c('f'), G => c('g'),
        H => c('h'), I => c('i'), J => c('j'), K => c('k'), L => c('l'), M => c('m'), N => c('n'),
        O => c('o'), P => c('p'), Q => c('q'), R => c('r'), S => c('s'), T => c('t'), U => c('u'),
        V => c('v'), W => c('w'), X => c('x'), Y => c('y'), Z => c('z'),
        Key0 => c('0'), Key1 => c('1'), Key2 => c('2'), Key3 => c('3'), Key4 => c('4'),
        Key5 => c('5'), Key6 => c('6'), Key7 => c('7'), Key8 => c('8'), Key9 => c('9'),
        Minus => c('-'), Equal => c('='),
        Space => Some(Key::Space),
        Enter | NumPadEnter => Some(Key::Enter),
        Escape => Some(Key::Esc),
        Tab => Some(Key::Tab),
        Backspace => Some(Key::Backspace),
        Up => Some(Key::Up),
        Down => Some(Key::Down),
        Left => Some(Key::Left),
        Right => Some(Key::Right),
        F1 => Some(Key::F(1)), F2 => Some(Key::F(2)), F3 => Some(Key::F(3)), F4 => Some(Key::F(4)),
        F5 => Some(Key::F(5)), F6 => Some(Key::F(6)), F7 => Some(Key::F(7)), F8 => Some(Key::F(8)),
        F9 => Some(Key::F(9)), F10 => Some(Key::F(10)), F11 => Some(Key::F(11)), F12 => Some(Key::F(12)),
        _ => None,
    }
}

impl Backend for WindowBackend {
    fn caps(&self) -> Caps {
        Caps { key_release: true, color: ColorMode::TrueColor, name: "window", font_zoom: true }
    }

    fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    fn poll(&mut self, input: &mut InputState) -> anyhow::Result<()> {
        if !self.window.is_open() {
            input.quit = true;
            return Ok(());
        }
        // Keys: diff the held set against last frame.
        let held: Vec<Key> = self.window.get_keys().into_iter().filter_map(map_key).collect();
        for k in &held {
            if !self.held_prev.contains(k) {
                input.key_down(*k);
            }
        }
        for k in &self.held_prev {
            if !held.contains(k) {
                input.key_up(*k);
            }
        }
        // Key repeat in menus: minifb reports repeats through get_keys_pressed.
        for k in self.window.get_keys_pressed(KeyRepeat::Yes).into_iter().filter_map(map_key) {
            if self.held_prev.contains(&k) && held.contains(&k) {
                input.pressed.push(k);
            }
        }
        self.held_prev = held;

        if let Some((mx, my)) = self.window.get_mouse_pos(MouseMode::Discard) {
            input.mouse = Some((mx / self.cell_w as f32, my / self.cell_h as f32));
        }
        for (i, b) in [MButton::Left, MButton::Right, MButton::Middle].into_iter().enumerate() {
            let down = self.window.get_mouse_down(b);
            let ours = [MouseButton::Left, MouseButton::Right, MouseButton::Middle][i];
            if down && !self.btn_prev[i] {
                input.mouse_down(ours);
            } else if !down && self.btn_prev[i] {
                input.mouse_up(ours);
            }
            self.btn_prev[i] = down;
        }
        if let Some((_, dy)) = self.window.get_scroll_wheel() {
            if dy.abs() > 0.01 {
                // Zoom = font size. Scrolling up makes text bigger (closer camera).
                let step = if dy > 0.0 { 1.0 } else { -1.0 };
                self.set_font_px(self.px + step);
                input.resized = true;
            }
        }
        let (w, h) = self.window.get_size();
        if (w, h) != (self.win_w, self.win_h) && w > 0 && h > 0 {
            self.win_w = w;
            self.win_h = h;
            self.relayout();
            input.resized = true;
        }
        if !self.window.is_active() {
            input.release_all();
            self.held_prev.clear();
            self.btn_prev = [false; 3];
        }
        Ok(())
    }

    fn present(&mut self, grid: &CellGrid) -> anyhow::Result<PresentStats> {
        let t0 = Instant::now();
        let full = self.front.len() != grid.cells.len();
        if full {
            self.front = vec![Cell { ch: '\u{0}', ..Cell::BLANK }; grid.cells.len()];
            self.buf.fill(0);
        }
        let mut changed = 0;
        for y in 0..grid.h as usize {
            for x in 0..grid.w as usize {
                let i = y * grid.w as usize + x;
                let cell = grid.cells[i];
                if cell != self.front[i] {
                    self.draw_cell(x, y, cell);
                    self.front[i] = cell;
                    changed += 1;
                }
            }
        }
        self.window.update_with_buffer(&self.buf, self.win_w, self.win_h)?;
        Ok(PresentStats { bytes: changed, write_us: t0.elapsed().as_micros() as u64 })
    }

    fn wait_until(&mut self, deadline: Instant) -> anyhow::Result<()> {
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
        Ok(())
    }

    fn invalidate(&mut self) {
        self.front.clear();
    }
}
