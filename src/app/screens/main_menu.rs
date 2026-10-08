//! Title screen.

use crate::app::{Ctx, Screen, Transition};
use crate::platform::audio::Sfx;
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::scene::Scene;
use crate::render::ui;
use crate::render::world::WorldLayer;

const ITEMS: [&str; 5] = ["New Run", "Gallery", "Settings", "Controls", "Quit"];

pub struct MainMenu {
    sel: usize,
    t: f32,
    rects: Vec<(i32, i32, i32)>,
    scene: Scene,
    layer: WorldLayer,
}

impl MainMenu {
    pub fn new() -> MainMenu {
        MainMenu { sel: 0, t: 0.0, rects: Vec::new(), scene: Scene::default(), layer: WorldLayer::default() }
    }

    fn activate(&self, ctx: &mut Ctx, i: usize) -> Transition {
        ctx.sfx(Sfx::UiSelect);
        match i {
            0 => Transition::Push(Box::new(super::class_select::ClassSelect::new(ctx))),
            1 => Transition::Push(Box::new(super::gallery::Gallery::new(ctx, None))),
            2 => Transition::Push(Box::new(super::settings::SettingsScreen::new())),
            3 => Transition::Push(Box::new(super::controls::Controls)),
            _ => Transition::Quit,
        }
    }
}

impl Default for MainMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for MainMenu {
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        self.t += dt;
        let n = ITEMS.len();
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
                if self.sel != i {
                    self.sel = i;
                }
                if input.clicked(MouseButton::Left) {
                    return self.activate(ctx, i);
                }
            }
        }
        if input.was_pressed(Key::Enter) || input.was_pressed(Key::Space) {
            return self.activate(ctx, self.sel);
        }
        if input.was_pressed(Key::Esc) {
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        let (w, h) = (grid.w as i32, grid.h as i32);
        // Title art, with a slow shimmer running across it.
        let lines: Vec<&str> = ctx.assets.title.lines().collect();
        let tw = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
        let tx = (w - tw) / 2;
        let ty = 2;
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let wave = ((c as f32 * 0.15 - self.t * 2.0 + r as f32 * 0.3).sin() * 0.5 + 0.5).powi(3);
                let col = Rgb(200, 150, 80).lerp(Rgb(255, 235, 170), wave);
                grid.put(tx + c as i32, ty + r as i32, ch, col);
            }
        }
        grid.text_centered(ty + lines.len() as i32 + 1, "an isometric ASCII dungeon crawler", ui::UI_DIM);

        // The four heroes, slowly turning.
        let order = ["knight", "wizard", "rogue", "archer"];
        let mut classes: Vec<(String, String)> = ctx.assets.defs.classes.iter().map(|(k, c)| (k.clone(), c.figure.clone())).collect();
        classes.sort_by_key(|(k, _)| order.iter().position(|o| o == k).unwrap_or(99));
        let n = classes.len() as i32;
        let spacing = 18;
        let fx0 = w / 2 - (n - 1) * spacing / 2;
        let fy = ty + lines.len() as i32 + 14;
        for (i, (_, fig)) in classes.iter().enumerate() {
            let angle = 0.6 + self.t * 0.5 + i as f32 * 0.7;
            super::draw_figure_at(ctx, &mut self.scene, &mut self.layer, grid, fig, fx0 + i as i32 * spacing, fy, angle, 1.0, "idle", self.t, true);
        }

        // Menu.
        let mw = 22;
        let mx = (w - mw) / 2;
        let my = (fy + 4).min(h - ITEMS.len() as i32 - 4);
        self.rects.clear();
        for (i, item) in ITEMS.iter().enumerate() {
            let y = my + i as i32 * 2;
            ui::menu_item(grid, mx, y, mw, item, i == self.sel);
            self.rects.push((mx, y, mw));
        }
        let hint = if ctx.caps.key_release {
            "keyboard: full (WASD available)"
        } else {
            "keyboard: press-only terminal — mouse movement mode (kitty / Ghostty recommended)"
        };
        grid.text_centered(h - 2, hint, Rgb(110, 110, 120));
    }
}
