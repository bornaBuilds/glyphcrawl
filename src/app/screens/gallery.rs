//! Asset gallery: every figure turns slowly (at game scale and enlarged), every stamp is
//! shown lit. Edit files under `assets/`, press F5, see the change.

use glam::Vec3;

use crate::app::{Ctx, Screen, Transition};
use crate::platform::{InputState, Key, MouseButton};
use crate::render::ascii_shader::FigureStyle;
use crate::render::cell::{CellGrid, Rgb};
use crate::render::figure::ClipPlay;
use crate::render::iso::{world_to_screen, Camera};
use crate::render::scene::{FigureInst, Scene};
use crate::render::world::{draw_stamp, LightSrc, StampDraw, WorldLayer};

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Figure(String),
    Stamp(String),
}

pub struct Gallery {
    sel: usize,
    clip: usize,
    spin: bool,
    angle: f32,
    t: f32,
    layer: WorldLayer,
    scene: Scene,
}

fn entries(ctx: &Ctx) -> Vec<Entry> {
    let mut v: Vec<Entry> = ctx.assets.figures.keys().map(|k| Entry::Figure(k.clone())).collect();
    v.extend(ctx.assets.stamps.stamps.iter().map(|s| Entry::Stamp(s.name.clone())));
    v
}

impl Gallery {
    pub fn new(ctx: &Ctx, focus: Option<&str>) -> Gallery {
        let sel = focus
            .and_then(|f| entries(ctx).iter().position(|e| matches!(e, Entry::Figure(n) | Entry::Stamp(n) if n == f)))
            .unwrap_or(0);
        Gallery { sel, clip: 0, spin: true, angle: 0.6, t: 0.0, layer: WorldLayer::default(), scene: Scene::default() }
    }

    /// Clip names with "idle" first, then "walk", then the rest alphabetically.
    fn clip_names(ctx: &Ctx, fig: &str) -> Vec<String> {
        let mut v: Vec<String> = ctx.assets.figures.get(fig).map(|f| f.clips.keys().cloned().collect()).unwrap_or_default();
        v.sort_by_key(|n| (n != "idle", n != "walk", n.clone()));
        v
    }
}

impl Screen for Gallery {
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        let list = entries(ctx);
        if input.back() {
            return Transition::Pop;
        }
        let n = list.len().max(1);
        if input.was_pressed(Key::Down) || input.was_pressed(Key::Char('s')) {
            self.sel = (self.sel + 1) % n;
            self.clip = 0;
            self.t = 0.0;
        }
        if input.was_pressed(Key::Up) || input.was_pressed(Key::Char('w')) {
            self.sel = (self.sel + n - 1) % n;
            self.clip = 0;
            self.t = 0.0;
        }
        if input.was_pressed(Key::Tab) {
            self.clip += 1;
            self.t = 0.0;
        }
        if input.was_pressed(Key::Space) {
            self.spin = !self.spin;
        }
        if input.clicked(MouseButton::Left)
            && let Some((mx, my)) = input.mouse_cell()
                && mx < 26 && my >= 3 {
                    let i = (my - 3) as usize;
                    if i < list.len() {
                        self.sel = i;
                        self.clip = 0;
                    }
                }
        self.t += dt;
        if self.spin {
            self.angle += dt * 0.7;
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        let list = entries(ctx);
        let (w, h) = (grid.w as i32, grid.h as i32);
        grid.text(2, 1, "GALLERY", Rgb(255, 210, 120));
        let max_rows = (h - 5).max(1) as usize;
        let scroll = self.sel.saturating_sub(max_rows - 1);
        for (row, (i, e)) in list.iter().enumerate().skip(scroll).take(max_rows).enumerate() {
            let (label, col) = match e {
                Entry::Figure(n) => (format!("◆ {n}"), Rgb(150, 210, 255)),
                Entry::Stamp(n) => (format!("▪ {n}"), Rgb(200, 190, 170)),
            };
            let y = 3 + row as i32;
            if i == self.sel {
                grid.text_bg(1, y, &format!(" {label:<23}"), Rgb::BLACK, Rgb(230, 200, 120));
            } else {
                grid.text(2, y, &label, col);
            }
        }
        grid.text(
            2,
            h - 1,
            "↑/↓ select · Tab animation · Space pause spin · F5 reload assets · Esc back",
            Rgb(140, 140, 150),
        );

        self.layer.reset(grid.w, grid.h);
        let light_pos = Vec3::new(2.0, 3.0, 2.5);
        let lights = [LightSrc { pos: light_pos, color: [1.4, 1.2, 1.0], radius: 9.0, mask: None }];
        let Some(entry) = list.get(self.sel).cloned() else { return };
        match entry {
            Entry::Figure(name) => {
                let Some(fig) = ctx.assets.figures.get(&name) else { return };
                let clips = Gallery::clip_names(ctx, &name);
                let clip_name = if clips.is_empty() { String::new() } else { clips[self.clip % clips.len()].clone() };
                let dur = fig.clips.get(&clip_name).map(|c| if c.looped { c.dur } else { c.dur + 0.6 }).unwrap_or(1.0);
                let t = self.t % dur.max(0.01);
                // Small floor patch under the game-scale figure.
                let center_x = 26 + (w - 26) / 3;
                let center_y = h / 2 + 4;
                let origin = Vec3::ZERO;
                let s0 = world_to_screen(origin);
                let cam = Camera { col: (s0.x - center_x as f32) as i32, row: (s0.y - center_y as f32) as i32 };
                let art = ctx.assets.world_art();
                let opts = StampDraw { visible: true, tile_idx: None, walkable: true, lights: &lights, ambient: [0.1, 0.1, 0.12], time: self.t };
                for j in -2..=1 {
                    for i in -2..=1 {
                        if let Some(&sid) = ctx.assets.tileset.floor.first() {
                            draw_stamp(grid, &mut self.layer, &cam, &art, &ctx.assets.stamps.stamps[sid], i, j, &opts);
                        }
                    }
                }
                let clip = ClipPlay { name: &clip_name, t, w: 1.0 };
                let small = {
                    let mut f = FigureInst::new(fig, Vec3::ZERO, self.angle);
                    f.clips.push(clip);
                    f.style = FigureStyle::default();
                    f
                };
                let a = &ctx.assets;
                self.scene.draw(grid, &self.layer, &cam, &[small], &lights, &a.mats, &a.shader);
                // Enlarged copy on the right (no floor, separate camera).
                let big_x = 26 + (w - 26) * 3 / 4;
                let big_y = h / 2 + 10;
                let cam2 = Camera { col: (s0.x - big_x as f32) as i32, row: (s0.y - big_y as f32) as i32 };
                let mut big = FigureInst::new(fig, Vec3::ZERO, self.angle);
                big.scale = 2.4;
                big.clips.push(ClipPlay { name: &clip_name, t, w: 1.0 });
                let empty = WorldLayer { w: grid.w, h: grid.h, height: vec![f32::NEG_INFINITY; grid.cells.len()], face: self.layer.face.clone(), floor: vec![false; grid.cells.len()] };
                let lights2 = [LightSrc { pos: Vec3::new(4.0, 6.0, 5.0), color: [1.4, 1.2, 1.0], radius: 14.0, mask: None }];
                self.scene.draw(grid, &empty, &cam2, &[big], &lights2, &a.mats, &a.shader);
                grid.text(28, 1, &format!("figure: {name}"), Rgb(150, 210, 255));
                grid.text(28, 2, &format!("animation: {clip_name}  ({} parts)", fig.parts.len()), Rgb(180, 180, 190));
            }
            Entry::Stamp(name) => {
                let Some(st) = ctx.assets.stamps.get(&name) else { return };
                let center_x = 26 + (w - 26) / 2;
                let center_y = h / 2;
                let s0 = world_to_screen(Vec3::ZERO);
                let cam = Camera { col: (s0.x - center_x as f32) as i32, row: (s0.y - center_y as f32) as i32 };
                let art = ctx.assets.world_art();
                let opts = StampDraw { visible: true, tile_idx: None, walkable: true, lights: &lights, ambient: [0.15, 0.15, 0.18], time: self.t };
                draw_stamp(grid, &mut self.layer, &cam, &art, st, 0, 0, &opts);
                grid.text(28, 1, &format!("stamp: {name}"), Rgb(200, 190, 170));
                grid.text(28, 2, &format!("file: {}  size {}x{}  {:?}", st.file, st.w, st.h, st.proxy), Rgb(160, 160, 170));
            }
        }
    }
}
