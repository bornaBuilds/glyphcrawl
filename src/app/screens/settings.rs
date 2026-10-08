//! Settings: ↑/↓ choose, ←/→ (or click) change. Saved when you leave.

use crate::app::settings::{ColorPref, FrameCap, InputMode};
use crate::app::{Ctx, Screen, Transition};
use crate::platform::audio::Sfx;
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::ui;

pub struct SettingsScreen {
    sel: usize,
    rects: Vec<(i32, i32, i32)>,
}

const ROWS: usize = 6;

impl SettingsScreen {
    pub fn new() -> SettingsScreen {
        SettingsScreen { sel: 0, rects: Vec::new() }
    }

    fn change(&self, ctx: &mut Ctx, row: usize, dir: i32) {
        let s = &mut ctx.settings;
        match row {
            0 => s.volume = ((s.volume * 10.0).round() + dir as f32).clamp(0.0, 10.0) / 10.0,
            1 => s.show_fps = !s.show_fps,
            2 => {
                s.input_mode = match s.input_mode {
                    InputMode::Auto => InputMode::MouseOnly,
                    InputMode::MouseOnly => InputMode::Auto,
                }
            }
            3 => {
                let all = [ColorPref::Auto, ColorPref::TrueColor, ColorPref::Ansi256];
                let i = all.iter().position(|c| *c == s.color).unwrap_or(0) as i32;
                s.color = all[(i + dir).rem_euclid(3) as usize];
            }
            4 => s.screen_shake = !s.screen_shake,
            _ => {
                let all = [FrameCap::Auto, FrameCap::Fps30, FrameCap::Fps60];
                let i = all.iter().position(|c| *c == s.frame_cap).unwrap_or(0) as i32;
                s.frame_cap = all[(i + dir).rem_euclid(3) as usize];
            }
        }
        ctx.settings_dirty = true;
        ctx.sfx(Sfx::UiMove);
    }

    fn rows(ctx: &Ctx) -> [(String, String); ROWS] {
        let s = &ctx.settings;
        [
            ("Volume".into(), format!("{:>3.0}%", s.volume * 100.0)),
            ("Show FPS".into(), if s.show_fps { "on" } else { "off" }.into()),
            (
                "Movement".into(),
                match s.input_mode {
                    InputMode::Auto => "auto (WASD when supported)",
                    InputMode::MouseOnly => "mouse only",
                }
                .into(),
            ),
            (
                "Colours".into(),
                match s.color {
                    ColorPref::Auto => "auto",
                    ColorPref::TrueColor => "24-bit",
                    ColorPref::Ansi256 => "256 colours",
                }
                .into(),
            ),
            ("Screen shake".into(), if s.screen_shake { "on" } else { "off" }.into()),
            (
                "Frame cap".into(),
                match s.frame_cap {
                    FrameCap::Auto => "auto (adapts to terminal)",
                    FrameCap::Fps30 => "30 fps",
                    FrameCap::Fps60 => "60 fps",
                }
                .into(),
            ),
        ]
    }
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for SettingsScreen {
    fn overlay(&self) -> bool {
        true
    }

    fn update(&mut self, ctx: &mut Ctx, input: &InputState, _dt: f32) -> Transition {
        if input.back() {
            let _ = ctx.settings.save();
            return Transition::Pop;
        }
        if input.was_pressed(Key::Down) || input.was_pressed(Key::Char('s')) {
            self.sel = (self.sel + 1) % ROWS;
        }
        if input.was_pressed(Key::Up) || input.was_pressed(Key::Char('w')) {
            self.sel = (self.sel + ROWS - 1) % ROWS;
        }
        if input.was_pressed(Key::Right) || input.was_pressed(Key::Char('d')) || input.was_pressed(Key::Enter) {
            self.change(ctx, self.sel, 1);
        }
        if input.was_pressed(Key::Left) || input.was_pressed(Key::Char('a')) {
            self.change(ctx, self.sel, -1);
        }
        let mouse = input.mouse_cell();
        for (i, &(x, y, w)) in self.rects.iter().enumerate() {
            if ui::hover(mouse, x, y, w, 1) {
                self.sel = i;
                if input.clicked(MouseButton::Left) {
                    self.change(ctx, i, 1);
                }
                if input.clicked(MouseButton::Right) {
                    self.change(ctx, i, -1);
                }
            }
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        grid.dim(0.3);
        let (w, h) = (grid.w as i32, grid.h as i32);
        let pw = 62;
        let ph = ROWS as i32 * 2 + 6;
        let x = (w - pw) / 2;
        let y = (h - ph) / 2;
        ui::panel(grid, x, y, pw, ph, ui::UI_GOLD, Rgb(14, 12, 16), Some("SETTINGS"));
        self.rects.clear();
        for (i, (label, value)) in Self::rows(ctx).iter().enumerate() {
            let yy = y + 2 + i as i32 * 2;
            let sel = i == self.sel;
            if sel {
                grid.text_bg(x + 2, yy, &" ".repeat((pw - 4) as usize), Rgb::BLACK, Rgb(40, 34, 24));
            }
            let bg = if sel { Rgb(40, 34, 24) } else { Rgb(14, 12, 16) };
            grid.text_bg(x + 4, yy, label, if sel { ui::UI_GOLD } else { ui::UI_TEXT }, bg);
            grid.text_bg(x + 22, yy, &format!("◂ {value} ▸"), ui::UI_TEXT, bg);
            self.rects.push((x + 2, yy, pw - 4));
        }
        grid.text(x + 3, y + ph - 2, "↑/↓ choose · ←/→ or click to change · Esc save & back", ui::UI_DIM);
        if !ctx.audio.is_real() {
            grid.text(x + 3, y + ph - 3, "(sound is off: built without the `audio` feature, or no device)", Rgb(150, 120, 90));
        }
    }
}
