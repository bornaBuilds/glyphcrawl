//! Native window backend: minifb for the window + input, fontdue to rasterize glyphs.
//! Cells are forced to be exactly twice as tall as they are wide so the isometric
//! projection matches the terminal. The mouse wheel changes the font size ("zoom").

use std::time::Instant;

use minifb::{Key as MKey, KeyRepeat, MouseButton as MButton, MouseMode, ScaleMode, Window, WindowOptions};

use super::glyph_atlas::GlyphRaster;
use super::{Backend, Caps, ColorMode, InputState, Key, MouseButton, PresentStats};
use crate::render::cell::{Cell, CellGrid};

const MIN_PX: f32 = 8.0;
const MAX_PX: f32 = 40.0;

pub struct WindowBackend {
    window: Window,
    raster: GlyphRaster,
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
        let raster = GlyphRaster::new(font_bytes, px.clamp(MIN_PX, MAX_PX))?;
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
            raster,
            buf: Vec::new(),
            win_w,
            win_h,
            cols: 0,
            rows: 0,
            front: Vec::new(),
            held_prev: Vec::new(),
            btn_prev: [false; 3],
        };
        wb.relayout();
        Ok(wb)
    }

    fn set_font_px(&mut self, px: f32) {
        self.raster.set_px(px.clamp(MIN_PX, MAX_PX));
        self.relayout();
    }

    fn relayout(&mut self) {
        self.buf = vec![0; self.win_w * self.win_h];
        self.cols = (self.win_w / self.raster.cell_w).max(1) as u16;
        self.rows = (self.win_h / self.raster.cell_h).max(1) as u16;
        self.front.clear();
    }
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
            input.mouse = Some((mx / self.raster.cell_w as f32, my / self.raster.cell_h as f32));
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
        if let Some((_, dy)) = self.window.get_scroll_wheel()
            && dy.abs() > 0.01 {
                // Zoom = font size. Scrolling up makes text bigger (closer camera).
                let step = if dy > 0.0 { 1.0 } else { -1.0 };
                self.set_font_px(self.raster.px + step);
                input.resized = true;
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
                    self.raster.draw_cell(&mut self.buf, self.win_w, self.win_h, x, y, cell);
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
