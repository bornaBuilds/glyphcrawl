//! The game itself: owns the `World`, turns input into `TickInput`, draws everything.

use glam::{Vec2, Vec3};

use crate::app::{Ctx, Screen, Transition};
use crate::game::level::Level;
use crate::game::world::{TickInput, World, HERO_LIGHT_RADIUS, TORCH_RADIUS};
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::iso::{screen_dir_to_world, Camera};
use crate::render::ascii_shader::FigureStyle;
use crate::render::figure::ClipPlay;
use crate::render::scene::{FigureInst, Scene};
use crate::render::world::{draw_world, flicker, LightSrc, WorldLayer, WorldView};

pub struct GameScreen {
    pub world: World,
    cam: Camera,
    layer: WorldLayer,
    scene: Scene,
    cam_init: bool,
    reveal: bool,
}

impl GameScreen {
    pub fn new(world: World) -> GameScreen {
        GameScreen { world, cam: Camera::default(), layer: WorldLayer::default(), scene: Scene::default(), cam_init: false, reveal: false }
    }

    /// A one-room level from a template name (dev mode).
    pub fn from_room(ctx: &Ctx, name: &str, seed: u64) -> anyhow::Result<GameScreen> {
        let idx = ctx.assets.room(name).ok_or_else(|| anyhow::anyhow!("no room template named '{name}'"))?;
        let level = Level::single_room(&ctx.assets.rooms, idx);
        Ok(GameScreen::new(World::new(level, seed)))
    }

    fn mouse_ground(&self, input: &InputState) -> Option<Vec2> {
        input.mouse.map(|(x, y)| self.cam.grid_to_plane(Vec2::new(x, y), 0.0))
    }

    fn tick_input(&self, ctx: &Ctx, input: &InputState) -> TickInput {
        let mut ti = TickInput::default();
        if ctx.wasd() {
            let mut right = 0.0;
            let mut down = 0.0;
            if input.is_held(Key::Char('w')) || input.is_held(Key::Up) {
                down -= 1.0;
            }
            if input.is_held(Key::Char('s')) || input.is_held(Key::Down) {
                down += 1.0;
            }
            if input.is_held(Key::Char('a')) || input.is_held(Key::Left) {
                right -= 1.0;
            }
            if input.is_held(Key::Char('d')) || input.is_held(Key::Right) {
                right += 1.0;
            }
            // Screen rows are twice as tall as columns are wide, so "up" covers more ground.
            ti.move_dir = screen_dir_to_world(right, down);
        }
        if let Some(g) = self.mouse_ground(input) {
            ti.aim = g;
            if input.btn_down[MouseButton::Left as usize] {
                ti.move_to = Some(g);
            }
        }
        ti
    }
}

impl Screen for GameScreen {
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        if input.was_pressed(Key::Esc) {
            return Transition::Quit;
        }
        if input.was_pressed(Key::F(2)) {
            self.reveal = !self.reveal;
        }
        let ti = self.tick_input(ctx, input);
        self.world.step(&ti, dt);
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        let hero = self.world.hero();
        let hero_p = Vec3::new(hero.pos.x, 0.0, hero.pos.y);
        if !self.cam_init {
            self.cam.center_on(hero_p, grid.w, grid.h);
            self.cam_init = true;
        } else {
            self.cam.follow(hero_p, grid.w, grid.h, (grid.w / 10) as i32, (grid.h / 8) as i32);
        }

        let time = self.world.time;
        let mut lights: Vec<LightSrc> = Vec::with_capacity(self.world.torches.len() + 1);
        lights.push(LightSrc {
            pos: hero_p + Vec3::new(0.0, 1.6, 0.0),
            color: [1.5, 1.25, 0.95],
            radius: HERO_LIGHT_RADIUS + 1.5,
            mask: Some(&self.world.hero_fov),
        });
        for t in &self.world.torches {
            let f = 0.85 + 0.25 * flicker(time, t.tile.x, t.tile.y);
            lights.push(LightSrc { pos: t.pos, color: [1.7 * f, 0.95 * f, 0.45 * f], radius: TORCH_RADIUS + 1.5, mask: Some(&t.mask) });
        }
        let all = vec![true; self.world.visible.len()];
        let view = WorldView {
            map: self.world.map(),
            visible: if self.reveal { &all } else { &self.world.visible },
            seen: if self.reveal { &all } else { &self.world.seen },
            lights: &lights,
            ambient: [0.06, 0.065, 0.1],
            time,
        };
        draw_world(grid, &mut self.layer, &self.cam, &view, &ctx.assets.world_art());

        let mut figs = Vec::new();
        if let Some(fig) = ctx.assets.figures.get(&hero.figure) {
            let mut f = FigureInst::new(fig, hero_p, hero.facing);
            f.clips.push(ClipPlay { name: if hero.moving { "walk" } else { "idle" }, t: time, w: 1.0 });
            let (ti, tj) = self.world.map().tile_at(hero.pos);
            f.style = FigureStyle { xray: Some(Rgb(80, 220, 255)), tile: Some(self.world.map().idx(ti, tj)), ..Default::default() };
            figs.push(f);
        }
        let a = &ctx.assets;
        self.scene.draw(grid, &self.layer, &self.cam, &figs, &lights, &a.mats, &a.shader);
        ctx.debug_line = format!("hero ({:.1},{:.1})", hero.pos.x, hero.pos.y);
    }
}
