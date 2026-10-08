//! Controls reference.

use crate::app::{Ctx, Screen, Transition};
use crate::platform::{InputState, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::ui;

pub struct Controls;

const LINES: [(&str, &str); 14] = [
    ("Left click", "walk there / attack the enemy under the cursor (hold to keep going)"),
    ("Right click  E", "special ability (hold to block / draw the bow)"),
    ("Q", "skill"),
    ("Space", "mobility (dash, blink, roll, charge)"),
    ("W A S D", "move (terminals with key-release support, or the window)"),
    ("  + Left click", "while moving with WASD: attack towards the cursor"),
    ("1 2 3 4", "the four abilities, keyboard only"),
    ("Esc", "pause"),
    ("", ""),
    ("F1", "debug overlay (fps, bytes per frame)"),
    ("F5", "reload everything in assets/ (art, rooms, balance)"),
    ("F2 / F3 / F4", "dev: reveal map / god mode / kill all enemies"),
    ("Zoom", "terminal: your font size (Ctrl - / Ctrl +) · window: mouse wheel"),
    ("Mouse mode", "if keys can't be held, click to move — kitty/Ghostty recommended"),
];

impl Screen for Controls {
    fn overlay(&self) -> bool {
        true
    }

    fn update(&mut self, _ctx: &mut Ctx, input: &InputState, _dt: f32) -> Transition {
        if input.back() || input.confirm() || input.clicked(MouseButton::Left) {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, _ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        grid.dim(0.3);
        let (w, h) = (grid.w as i32, grid.h as i32);
        let pw = 96.min(w - 4);
        let ph = LINES.len() as i32 + 5;
        let x = (w - pw) / 2;
        let y = (h - ph) / 2;
        ui::panel(grid, x, y, pw, ph, ui::UI_GOLD, Rgb(14, 12, 16), Some("CONTROLS"));
        for (i, (k, v)) in LINES.iter().enumerate() {
            grid.text(x + 3, y + 2 + i as i32, k, ui::UI_GOLD);
            grid.text(x + 22, y + 2 + i as i32, v, ui::UI_TEXT);
        }
        grid.text(x + 3, y + ph - 2, "any key to close", ui::UI_DIM);
    }
}
