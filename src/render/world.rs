//! Draws the dungeon: every visible tile's stamps, lit by point lights, with fog of war.
//! Also fills `WorldLayer` — the per-cell height buffer the 3D figures are depth-tested against.

use glam::{Vec2, Vec3};
use serde::Deserialize;

use super::cell::{Cell, CellGrid, Rgb};
use super::iso::Camera;
use super::material::{GlyphDim, MaterialLib};
use super::stamp::{Face, Stamp, StampLib};
use crate::game::tile::{Tile, TileMap};

/// Per-cell data about the dungeon surface under each cell.
#[derive(Clone, Debug, Default)]
pub struct WorldLayer {
    pub w: u16,
    pub h: u16,
    /// World height (y) of the visible surface; -inf where nothing is drawn.
    pub height: Vec<f32>,
    pub face: Vec<Face>,
    /// Walkable floor at ground level (ground decals may be drawn here).
    pub floor: Vec<bool>,
}

impl WorldLayer {
    pub fn reset(&mut self, w: u16, h: u16) {
        let n = w as usize * h as usize;
        self.w = w;
        self.h = h;
        self.height.clear();
        self.height.resize(n, f32::NEG_INFINITY);
        self.face.clear();
        self.face.resize(n, Face::Flat);
        self.floor.clear();
        self.floor.resize(n, false);
    }

    /// Surface height at a sub-cell sample position (fractional grid coords), correcting for
    /// the slope of vertical faces within the cell.
    #[inline]
    pub fn height_at(&self, idx: usize, frac: Vec2) -> f32 {
        let h = self.height[idx];
        match self.face[idx] {
            Face::Left => h - (frac.y - 0.5) / super::iso::ROWS_PER_Y + (frac.x - 0.5) / (4.0 * super::iso::ROWS_PER_Y),
            Face::Right => h - (frac.y - 0.5) / super::iso::ROWS_PER_Y - (frac.x - 0.5) / (4.0 * super::iso::ROWS_PER_Y),
            _ => h,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LightSrc<'a> {
    pub pos: Vec3,
    pub color: [f32; 3],
    pub radius: f32,
    /// Tiles this light can reach (its field of view). None = everything in radius.
    pub mask: Option<&'a [bool]>,
}

/// Sum of all light arriving at point `p` with normal `n` on tile `tile`.
pub fn shade(p: Vec3, n: Vec3, tile: Option<usize>, lights: &[LightSrc], ambient: [f32; 3]) -> [f32; 3] {
    let mut c = ambient;
    for l in lights {
        if let (Some(m), Some(t)) = (l.mask, tile) {
            if !m.get(t).copied().unwrap_or(false) {
                continue;
            }
        }
        let d = l.pos - p;
        let dist2 = d.length_squared();
        let r2 = l.radius * l.radius;
        if dist2 >= r2 {
            continue;
        }
        let dist = dist2.sqrt().max(1e-3);
        // Smooth falloff that stays bright near the source and fades to zero at the radius.
        let x = dist / l.radius;
        let att = (1.0 - x * x).powi(2) / (1.0 + 2.0 * x * x);
        let ndl = (n.dot(d / dist)).max(0.0) * 0.7 + 0.3;
        let k = att * ndl;
        c[0] += l.color[0] * k;
        c[1] += l.color[1] * k;
        c[2] += l.color[2] * k;
    }
    c
}

/// Exposure tone curve: lit albedo -> display value.
#[inline]
pub fn tone(v: f32) -> f32 {
    1.0 - (-2.0 * v).exp()
}

pub fn tone_rgb(albedo: Rgb, light: [f32; 3]) -> Rgb {
    let a = albedo.to_f32();
    Rgb::from_f32([tone(a[0] * light[0] * 2.0), tone(a[1] * light[1] * 2.0), tone(a[2] * light[2] * 2.0)])
}

/// How many dimming steps a glyph gets at this light level.
pub fn dim_steps(light: [f32; 3]) -> u32 {
    let l = light[0].max(light[1]).max(light[2]);
    if l > 0.30 {
        0
    } else if l > 0.16 {
        1
    } else if l > 0.08 {
        2
    } else if l > 0.035 {
        3
    } else {
        9
    }
}

/// Which stamps draw each tile type (from `assets/tileset.ron`).
#[derive(Clone, Debug, Deserialize)]
pub struct TilesetDef {
    pub floor: Vec<String>,
    pub wall: Vec<String>,
    pub stump: Vec<String>,
    pub wall_torch: String,
    pub stump_torch: String,
    pub door_open: String,
    pub door_closed: String,
    pub door_closed_stump: String,
    pub pit: String,
    pub pillar: String,
    pub brazier: String,
    pub chest: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tileset {
    pub floor: Vec<usize>,
    pub wall: Vec<usize>,
    pub stump: Vec<usize>,
    pub wall_torch: usize,
    pub stump_torch: usize,
    pub door_open: usize,
    pub door_closed: usize,
    pub door_closed_stump: usize,
    pub pit: usize,
    pub pillar: usize,
    pub brazier: usize,
    pub chest: usize,
}

impl Tileset {
    pub fn resolve(def: &TilesetDef, stamps: &StampLib) -> Result<Tileset, String> {
        let one = |n: &String| stamps.id(n).ok_or_else(|| format!("tileset: unknown stamp '{n}'"));
        // Entries may carry a weight: "floor_a*6".
        let many = |v: &Vec<String>| -> Result<Vec<usize>, String> {
            let mut out = Vec::new();
            for e in v {
                let (name, w) = match e.split_once('*') {
                    Some((n, w)) => (n.trim().to_string(), w.trim().parse::<usize>().map_err(|_| format!("tileset: bad weight in '{e}'"))?),
                    None => (e.clone(), 1),
                };
                let id = one(&name)?;
                out.extend(std::iter::repeat_n(id, w.max(1)));
            }
            if out.is_empty() {
                return Err("tileset: empty stamp list".into());
            }
            Ok(out)
        };
        Ok(Tileset {
            floor: many(&def.floor)?,
            wall: many(&def.wall)?,
            stump: many(&def.stump)?,
            wall_torch: one(&def.wall_torch)?,
            stump_torch: one(&def.stump_torch)?,
            door_open: one(&def.door_open)?,
            door_closed: one(&def.door_closed)?,
            door_closed_stump: one(&def.door_closed_stump)?,
            pit: one(&def.pit)?,
            pillar: one(&def.pillar)?,
            brazier: one(&def.brazier)?,
            chest: one(&def.chest)?,
        })
    }

    /// Stamps for a tile, bottom layer first (at most 2).
    pub fn stamps_for(&self, map: &TileMap, i: i32, j: i32) -> [Option<usize>; 2] {
        let h = TileMap::hash(i, j) as usize;
        let pick = |v: &Vec<usize>| v[h % v.len()];
        let floor = Some(pick(&self.floor));
        match map.get(i, j) {
            Tile::Void => [None, None],
            Tile::Floor => [floor, None],
            Tile::Wall => {
                if map.is_front_wall(i, j) {
                    [Some(pick(&self.stump)), None]
                } else {
                    [Some(pick(&self.wall)), None]
                }
            }
            Tile::WallTorch => {
                if map.is_front_wall(i, j) {
                    [Some(self.stump_torch), None]
                } else {
                    [Some(self.wall_torch), None]
                }
            }
            Tile::DoorOpen => [Some(self.door_open), None],
            Tile::DoorClosed => {
                if map.is_front_wall(i, j) {
                    [Some(self.door_closed_stump), None]
                } else {
                    [Some(self.door_closed), None]
                }
            }
            Tile::Pit => [Some(self.pit), None],
            Tile::Pillar => [floor, Some(self.pillar)],
            Tile::Brazier => [floor, Some(self.brazier)],
            Tile::Chest => [floor, Some(self.chest)],
        }
    }
}

pub struct WorldView<'a> {
    pub map: &'a TileMap,
    pub visible: &'a [bool],
    pub seen: &'a [bool],
    pub lights: &'a [LightSrc<'a>],
    pub ambient: [f32; 3],
    pub time: f32,
}

pub struct WorldArt<'a> {
    pub mats: &'a MaterialLib,
    pub stamps: &'a StampLib,
    pub tileset: &'a Tileset,
    pub dim: &'a GlyphDim,
}

/// Visible tile range for the current camera (with margin for tall stamps).
pub fn tile_range(cam: &Camera, w: u16, h: u16) -> (i32, i32, i32, i32) {
    let corners = [
        Vec2::new(-16.0, -6.0),
        Vec2::new(w as f32 + 16.0, -6.0),
        Vec2::new(-16.0, h as f32 + 16.0),
        Vec2::new(w as f32 + 16.0, h as f32 + 16.0),
    ];
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for c in corners {
        let g = cam.grid_to_plane(c, 0.0);
        lo = lo.min(g);
        hi = hi.max(g);
    }
    (lo.x.floor() as i32 - 1, lo.y.floor() as i32 - 1, hi.x.ceil() as i32 + 1, hi.y.ceil() as i32 + 1)
}

pub fn draw_world(grid: &mut CellGrid, layer: &mut WorldLayer, cam: &Camera, view: &WorldView, art: &WorldArt) {
    layer.reset(grid.w, grid.h);
    let map = view.map;
    let (i0, j0, i1, j1) = tile_range(cam, grid.w, grid.h);
    let i0 = i0.max(0);
    let j0 = j0.max(0);
    let i1 = i1.min(map.w - 1);
    let j1 = j1.min(map.h - 1);
    // Back-to-front order: by i + j, then i.
    let mut order: Vec<(i32, i32)> = Vec::new();
    for j in j0..=j1 {
        for i in i0..=i1 {
            let k = map.idx(i, j);
            if view.seen[k] {
                order.push((i, j));
            }
        }
    }
    order.sort_by_key(|&(i, j)| (i + j, i));
    for layer_no in 0..2 {
        for &(i, j) in &order {
            if let Some(sid) = art.tileset.stamps_for(map, i, j)[layer_no] {
                draw_stamp_on_tile(grid, layer, cam, view, art, &art.stamps.stamps[sid], i, j);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_stamp_on_tile(
    grid: &mut CellGrid,
    layer: &mut WorldLayer,
    cam: &Camera,
    view: &WorldView,
    art: &WorldArt,
    stamp: &Stamp,
    i: i32,
    j: i32,
) {
    let map = view.map;
    let tile_idx = map.idx(i, j);
    let tile = map.get(i, j);
    let opts = StampDraw {
        visible: view.visible[tile_idx],
        tile_idx: Some(tile_idx),
        walkable: tile.is_open() && tile != Tile::Pit,
        lights: view.lights,
        ambient: view.ambient,
        time: view.time,
    };
    draw_stamp(grid, layer, cam, art, stamp, i, j, &opts);
}

pub struct StampDraw<'a> {
    pub visible: bool,
    pub tile_idx: Option<usize>,
    pub walkable: bool,
    pub lights: &'a [LightSrc<'a>],
    pub ambient: [f32; 3],
    pub time: f32,
}

/// Draw one stamp on tile (i, j), lit, with the height test against `layer`.
#[allow(clippy::too_many_arguments)]
pub fn draw_stamp(grid: &mut CellGrid, layer: &mut WorldLayer, cam: &Camera, art: &WorldArt, stamp: &Stamp, i: i32, j: i32, o: &StampDraw) {
    let (tc, tr) = super::iso::tile_top(i, j);
    let gx0 = tc - cam.col - stamp.anchor.0;
    let gy0 = tr - cam.row - stamp.anchor.1;
    if gx0 + stamp.w < 0 || gy0 + stamp.h < 0 || gx0 >= grid.w as i32 || gy0 >= grid.h as i32 {
        return;
    }
    let base = Vec3::new(i as f32, 0.0, j as f32);
    for r in 0..stamp.h {
        let gy = gy0 + r;
        if gy < 0 || gy >= grid.h as i32 {
            continue;
        }
        for c in 0..stamp.w {
            let gx = gx0 + c;
            if gx < 0 || gx >= grid.w as i32 {
                continue;
            }
            let Some(sc) = stamp.cell(c, r) else { continue };
            let idx = grid.idx(gx, gy);
            let h = sc.pos.y;
            if h < layer.height[idx] - 1e-3 {
                continue;
            }
            let mat = art.mats.get(sc.mat);
            let (ch, fg) = if o.visible {
                let world = base + sc.pos;
                let lit = shade(world, sc.face.normal(), o.tile_idx, o.lights, o.ambient);
                if mat.emissive > 0.0 {
                    let flick = 0.85 + 0.15 * flicker(o.time, i, j);
                    (sc.ch, tone_rgb(mat.color, lit).lerp(mat.color.scale(flick), mat.emissive))
                } else {
                    (art.dim.dim(sc.ch, dim_steps(lit)), tone_rgb(mat.color, lit))
                }
            } else {
                // Remembered but not currently seen: cold, dim, one glyph step sparser.
                // Floors fade out entirely; walls keep a faint outline, like a map.
                let g = mat.color.luma();
                let c = Rgb((g * 55.0) as u8 + 10, (g * 62.0) as u8 + 12, (g * 85.0) as u8 + 22);
                (art.dim.dim(sc.ch, 2), c)
            };
            grid.cells[idx] = Cell { ch, fg, bg: Rgb::BLACK };
            layer.height[idx] = h;
            layer.face[idx] = sc.face;
            layer.floor[idx] = o.walkable && h < 0.01 && o.visible;
        }
    }
}

/// Torch flicker in 0..1, quantised to ~8 Hz so it doesn't flood the terminal with updates.
pub fn flicker(time: f32, i: i32, j: i32) -> f32 {
    let step = (time * 8.0) as u32;
    let h = TileMap::hash(i.wrapping_add(step as i32 * 7919), j) % 1000;
    h as f32 / 1000.0
}
