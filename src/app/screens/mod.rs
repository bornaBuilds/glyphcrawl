//! All screens: menus, the game itself, the gallery, and the M0 sandbox.

pub mod class_select;
pub mod controls;
pub mod gallery;
pub mod game;
pub mod main_menu;
pub mod pause;
pub mod sandbox;
pub mod settings;

use glam::Vec3;

use crate::app::Ctx;
use crate::render::cell::CellGrid;
use crate::render::figure::ClipPlay;
use crate::render::iso::{world_to_screen, Camera};
use crate::render::scene::{FigureInst, Scene};
use crate::render::world::{draw_stamp, LightSrc, StampDraw, WorldLayer};

/// Draw a single figure standing at grid cell (gx, gy) — for menus and previews.
#[allow(clippy::too_many_arguments)]
pub fn draw_figure_at(
    ctx: &Ctx,
    scene: &mut Scene,
    layer: &mut WorldLayer,
    grid: &mut CellGrid,
    name: &str,
    gx: i32,
    gy: i32,
    facing: f32,
    scale: f32,
    clip: &str,
    t: f32,
    floor: bool,
) {
    let Some(fig) = ctx.assets.figures.get(name) else { return };
    let s0 = world_to_screen(Vec3::ZERO);
    let cam = Camera { col: (s0.x - gx as f32) as i32, row: (s0.y - gy as f32) as i32 };
    let lights = [
        LightSrc { pos: Vec3::new(1.5, 3.0, 2.5), color: [1.3, 1.05, 0.8], radius: 9.0, mask: None },
        LightSrc { pos: Vec3::new(-2.0, 2.0, -1.0), color: [0.25, 0.3, 0.6], radius: 6.0, mask: None },
    ];
    layer.reset(grid.w, grid.h);
    if floor {
        let art = ctx.assets.world_art();
        let opts = StampDraw { visible: true, tile_idx: None, walkable: true, lights: &lights, ambient: [0.05, 0.05, 0.07], time: t };
        for j in -1i32..=0 {
            for i in -1i32..=0 {
                let n = ctx.assets.tileset.floor.len().max(1);
                if let Some(&sid) = ctx.assets.tileset.floor.get(((i + 3 * j).unsigned_abs() as usize) % n) {
                    draw_stamp(grid, layer, &cam, &art, &ctx.assets.stamps.stamps[sid], i, j, &opts);
                }
            }
        }
    }
    let mut f = FigureInst::new(fig, Vec3::ZERO, facing);
    f.scale = scale;
    f.clips.push(ClipPlay { name: clip, t, w: 1.0 });
    // Only the figure is depth-tested against nothing, so the floor never hides its feet.
    let empty = WorldLayer { w: grid.w, h: grid.h, height: vec![f32::NEG_INFINITY; grid.cells.len()], face: layer.face.clone(), floor: layer.floor.clone() };
    scene.draw(grid, &empty, &cam, &[f], &lights, &ctx.assets.mats, &ctx.assets.shader);
}

/// A seed from the wall clock (menus only — the simulation never reads clocks).
pub fn fresh_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) % 1_000_000
}
