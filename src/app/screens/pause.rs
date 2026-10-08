//! Pause overlay.

use crate::app::{Ctx, Screen, Transition};
use crate::platform::audio::Sfx;
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::ui;

const ITEMS: [&str; 5] = ["Resume", "Settings", "Controls", "Abandon run", "Quit game"];

pub struct Pause {
    sel: usize,
    rects: Vec<(i32, i32, i32)>,
}

impl Pause {
    pub fn new() -> Pause {
        Pause { sel: 0, rects: Vec::new() }
    }

    fn activate(&self, ctx: &mut Ctx, i: usize) -> Transition {
        ctx.sfx(Sfx::UiSelect);
        match i {
            0 => Transition::Pop,
            1 => Transition::Push(Box::new(super::settings::SettingsScreen::new())),
            2 => Transition::Push(Box::new(super::controls::Controls)),
            3 => Transition::Reset(Box::new(super::main_menu::MainMenu::new())),
            _ => Transition::Quit,
        }
    }
}

impl Default for Pause {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for Pause {
    fn overlay(&self) -> bool {
        true
    }

    fn update(&mut self, ctx: &mut Ctx, input: &InputState, _dt: f32) -> Transition {
        let n = ITEMS.len();
        if input.was_pressed(Key::Esc) {
            return Transition::Pop;
        }
        if input.was_pressed(Key::Down) || input.was_pressed(Key::Char('s')) {
            self.sel = (self.sel + 1) % n;
            ctx.sfx(Sfx::UiMove);
        }
        if input.was_pressed(Key::Up) || input.was_pressed(Key::Char('w')) {
            self.sel = (self.sel + n - 1) % n;
            ctx.sfx(Sfx::UiMove);
        }
        let mouse = input.mouse_cell();
        for (i, &(x, y, w)) in self.rects.iter().enumerate() {
            if ui::hover(mouse, x, y, w, 1) {
                self.sel = i;
                if input.clicked(MouseButton::Left) {
                    return self.activate(ctx, i);
                }
            }
        }
        if input.was_pressed(Key::Enter) || input.was_pressed(Key::Space) {
            return self.activate(ctx, self.sel);
        }
        Transition::None
    }

    fn render(&mut self, _ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        grid.dim(0.4);
        let (w, h) = (grid.w as i32, grid.h as i32);
        let pw = 30;
        let ph = ITEMS.len() as i32 * 2 + 4;
        let x = (w - pw) / 2;
        let y = (h - ph) / 2;
        ui::panel(grid, x, y, pw, ph, ui::UI_GOLD, Rgb(14, 12, 16), Some("PAUSED"));
        self.rects.clear();
        for (i, item) in ITEMS.iter().enumerate() {
            let yy = y + 2 + i as i32 * 2;
            ui::menu_item(grid, x + 2, yy, pw - 4, item, i == self.sel);
            self.rects.push((x + 2, yy, pw - 4));
        }
    }
}
