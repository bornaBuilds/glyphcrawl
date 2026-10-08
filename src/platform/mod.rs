//! Backends: everything that touches a real terminal, window or sound card lives here.
//! The rest of the game only sees `Backend`, `InputState` and `CellGrid`.

pub mod audio;
pub mod headless;
pub mod term_writer;
#[cfg(feature = "terminal")]
pub mod terminal;
#[cfg(feature = "window")]
pub mod window;

use std::collections::BTreeSet;
use std::time::Instant;

use crate::render::cell::CellGrid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
}

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    /// The backend reports key releases, so held keys (WASD) work.
    pub key_release: bool,
    pub color: ColorMode,
    pub name: &'static str,
    /// Window backend: zoom is handled by changing the font size.
    pub font_zoom: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PresentStats {
    pub bytes: usize,
    pub write_us: u64,
}

pub trait Backend {
    fn caps(&self) -> Caps;
    fn size(&self) -> (u16, u16);
    /// Drain all pending input without blocking.
    fn poll(&mut self, input: &mut InputState) -> anyhow::Result<()>;
    fn present(&mut self, grid: &CellGrid) -> anyhow::Result<PresentStats>;
    /// Sleep until `deadline`, waking early if input arrives.
    fn wait_until(&mut self, deadline: Instant) -> anyhow::Result<()>;
    /// Force the next present to redraw everything.
    fn invalidate(&mut self) {}
    fn set_color_mode(&mut self, _mode: ColorMode) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Space,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
}

/// Raw input state, filled by the backend. Edges (pressed/released) are latched until
/// `clear_edges` is called after a simulation tick consumed them.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    pub held: BTreeSet<Key>,
    pub pressed: Vec<Key>,
    pub released: Vec<Key>,
    /// Mouse position in cell coordinates (fractional; cell centre = x + 0.5).
    pub mouse: Option<(f32, f32)>,
    pub btn_down: [bool; 3],
    pub btn_pressed: [bool; 3],
    pub btn_released: [bool; 3],
    pub wheel: i32,
    pub resized: bool,
    pub quit: bool,
    /// Fallback for terminals without key-release events: when each held key expires.
    pub held_expiry: Vec<(Key, Instant)>,
}

impl InputState {
    pub fn key_down(&mut self, k: Key) {
        if self.held.insert(k) {
            self.pressed.push(k);
        }
    }

    pub fn key_up(&mut self, k: Key) {
        if self.held.remove(&k) {
            self.released.push(k);
        }
    }

    /// A press from a backend that never sends releases: the key counts as held until
    /// `until`, refreshed by auto-repeat.
    pub fn key_pulse(&mut self, k: Key, until: Instant) {
        // Without release events we can't tell a repeat from a new press, so every press
        // is an edge (menus then get key-repeat for free).
        self.held.insert(k);
        self.pressed.push(k);
        if let Some(e) = self.held_expiry.iter_mut().find(|(kk, _)| *kk == k) {
            e.1 = until;
        } else {
            self.held_expiry.push((k, until));
        }
    }

    pub fn expire_pulses(&mut self, now: Instant) {
        let mut expired = Vec::new();
        self.held_expiry.retain(|(k, t)| {
            if *t <= now {
                expired.push(*k);
                false
            } else {
                true
            }
        });
        for k in expired {
            self.key_up(k);
        }
    }

    pub fn mouse_down(&mut self, b: MouseButton) {
        let i = b as usize;
        if !self.btn_down[i] {
            self.btn_down[i] = true;
            self.btn_pressed[i] = true;
        }
    }

    pub fn mouse_up(&mut self, b: MouseButton) {
        let i = b as usize;
        if self.btn_down[i] {
            self.btn_down[i] = false;
            self.btn_released[i] = true;
        }
    }

    /// Release everything (focus lost).
    pub fn release_all(&mut self) {
        let keys: Vec<Key> = self.held.iter().copied().collect();
        for k in keys {
            self.key_up(k);
        }
        self.held_expiry.clear();
        for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            self.mouse_up(b);
        }
    }

    pub fn clear_edges(&mut self) {
        self.pressed.clear();
        self.released.clear();
        self.btn_pressed = [false; 3];
        self.btn_released = [false; 3];
        self.wheel = 0;
    }

    pub fn was_pressed(&self, k: Key) -> bool {
        self.pressed.contains(&k)
    }

    pub fn is_held(&self, k: Key) -> bool {
        self.held.contains(&k)
    }

    pub fn clicked(&self, b: MouseButton) -> bool {
        self.btn_pressed[b as usize]
    }

    pub fn mouse_cell(&self) -> Option<(i32, i32)> {
        self.mouse.map(|(x, y)| (x.floor() as i32, y.floor() as i32))
    }

    /// Any "confirm" style press: Enter, Space or left click.
    pub fn confirm(&self) -> bool {
        self.was_pressed(Key::Enter) || self.was_pressed(Key::Space)
    }

    pub fn back(&self) -> bool {
        self.was_pressed(Key::Esc) || self.was_pressed(Key::Backspace)
    }
}

/// Map a colour to the xterm-256 palette (6x6x6 cube + 24 greys).
pub fn rgb_to_ansi256(c: crate::render::cell::Rgb) -> u8 {
    let (r, g, b) = (c.0 as i32, c.1 as i32, c.2 as i32);
    const STEPS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let q = |v: i32| -> usize {
        let mut best = 0;
        for (i, s) in STEPS.iter().enumerate() {
            if (v - s).abs() < (v - STEPS[best]).abs() {
                best = i;
            }
        }
        best
    };
    let (qr, qg, qb) = (q(r), q(g), q(b));
    let cube = (STEPS[qr], STEPS[qg], STEPS[qb]);
    let cube_err = (r - cube.0).pow(2) + (g - cube.1).pow(2) + (b - cube.2).pow(2);
    let avg = (r + g + b) / 3;
    let gi = ((avg - 8).max(0) / 10).min(23);
    let gv = 8 + gi * 10;
    let grey_err = (r - gv).pow(2) + (g - gv).pow(2) + (b - gv).pow(2);
    if grey_err < cube_err {
        232 + gi as u8
    } else {
        16 + (36 * qr + 6 * qg + qb) as u8
    }
}

/// Approximate colour of an xterm-256 index (used to keep 256-colour output stable).
pub fn ansi256_to_rgb(i: u8) -> crate::render::cell::Rgb {
    use crate::render::cell::Rgb;
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    if i >= 232 {
        let v = 8 + (i - 232) * 10;
        Rgb(v, v, v)
    } else if i >= 16 {
        let n = i - 16;
        Rgb(STEPS[(n / 36) as usize], STEPS[((n / 6) % 6) as usize], STEPS[(n % 6) as usize])
    } else {
        Rgb(128, 128, 128)
    }
}

/// Pick the colour mode from the environment (COLORTERM).
pub fn detect_color_mode() -> ColorMode {
    match std::env::var("COLORTERM") {
        Ok(v) if v.contains("truecolor") || v.contains("24bit") => ColorMode::TrueColor,
        _ => match std::env::var("TERM_PROGRAM") {
            Ok(p) if ["ghostty", "WezTerm", "iTerm.app", "vscode"].contains(&p.as_str()) => {
                ColorMode::TrueColor
            }
            _ => match std::env::var("TERM") {
                Ok(t) if t.contains("kitty") || t.contains("ghostty") || t.contains("direct") => {
                    ColorMode::TrueColor
                }
                _ => ColorMode::Ansi256,
            },
        },
    }
}
