//! M0 platform test: a colour gradient, an `@` moved with WASD / click, a mouse reticle and
//! a readout of held keys. Reachable with `--screen sandbox`.

use crate::app::{Ctx, Screen, Transition};
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};

pub struct Sandbox {
    x: f32,
    y: f32,
    target: Option<(f32, f32)>,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        Sandbox { x: 20.0, y: 10.0, target: None }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for Sandbox {
    fn update(&mut self, _ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        if input.was_pressed(Key::Esc) {
            return Transition::Quit;
        }
        let speed = 30.0;
        let mut dx = 0.0;
        let mut dy = 0.0;
        if input.is_held(Key::Char('w')) {
            dy -= 1.0;
        }
        if input.is_held(Key::Char('s')) {
            dy += 1.0;
        }
        if input.is_held(Key::Char('a')) {
            dx -= 1.0;
        }
        if input.is_held(Key::Char('d')) {
            dx += 1.0;
        }
        if dx != 0.0 || dy != 0.0 {
            self.target = None;
            self.x += dx * speed * dt;
            self.y += dy * speed * 0.5 * dt;
        }
        if input.btn_down[MouseButton::Left as usize] {
            self.target = input.mouse;
        }
        if let Some((tx, ty)) = self.target {
            let (vx, vy) = (tx - self.x, (ty - self.y) * 2.0);
            let d = (vx * vx + vy * vy).sqrt();
            if d < 0.5 {
                self.target = None;
            } else {
                self.x += vx / d * speed * dt;
                self.y += vy / d * speed * 0.5 * dt;
            }
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, input: &InputState) {
        let (w, h) = (grid.w as i32, grid.h as i32);
        let ramp: Vec<char> = " .:-=+*#%@".chars().collect();
        for y in 0..h {
            for x in 0..w {
                let t = x as f32 / w as f32;
                let v = y as f32 / h as f32;
                let i = ((t * 0.5 + v * 0.5) * (ramp.len() - 1) as f32) as usize;
                grid.put(x, y, ramp[i], Rgb((t * 255.0) as u8, (v * 200.0) as u8, 160));
            }
        }
        grid.text(2, 1, "glyphcrawl M0 sandbox — WASD or click to move, Esc quits, F1 debug", Rgb::WHITE);
        let held: Vec<String> = input.held.iter().map(|k| format!("{k:?}")).collect();
        grid.text(2, 2, &format!("held: {}", held.join(" ")), Rgb(255, 230, 120));
        grid.text(
            2,
            3,
            &format!(
                "backend: {}  key releases: {}  wasd: {}",
                ctx.caps.name,
                ctx.caps.key_release,
                ctx.wasd()
            ),
            Rgb(180, 220, 255),
        );
        if !ctx.wasd() {
            grid.text(2, 4, "No key-release events: mouse-move mode (WASD will stutter)", Rgb(255, 120, 120));
        }
        grid.put(self.x as i32, self.y as i32, '@', Rgb(255, 255, 0));
        if let Some((mx, my)) = input.mouse_cell() {
            grid.put(mx, my, '+', Rgb(255, 80, 80));
        }
    }
}
