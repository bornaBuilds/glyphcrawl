//! The simulation state of one run and its fixed-timestep update.

use glam::{IVec2, Vec2, Vec3};
use slotmap::SlotMap;

use super::actor::{Actor, ActorId};
use super::fov;
use super::level::Level;
use super::rng::Rng;
use super::tile::{collide_circle, TileMap};

/// How far the hero can see in the dark (tiles) and how far their own light reaches.
pub const SIGHT_RADIUS: i32 = 16;
pub const HERO_LIGHT_RADIUS: f32 = 8.0;
pub const TORCH_RADIUS: f32 = 7.0;

/// What the player wants this tick, already translated from keys/mouse to world terms.
#[derive(Clone, Debug, Default)]
pub struct TickInput {
    /// WASD direction on the ground plane (zero = none).
    pub move_dir: Vec2,
    /// Click-to-move destination.
    pub move_to: Option<Vec2>,
    /// Ground point under the mouse.
    pub aim: Vec2,
    /// Enemy under the mouse when attacking.
    pub target: Option<ActorId>,
    /// Actions: 0 = basic (LMB), 1 = special (RMB), 2 = skill (Q), 3 = mobility (Space).
    pub act_pressed: [bool; 4],
    pub act_held: [bool; 4],
    pub act_released: [bool; 4],
    /// Reward choice (index into the offered upgrades).
    pub pick: Option<usize>,
}

pub struct Torch {
    pub tile: IVec2,
    pub pos: Vec3,
    /// Tiles this torch lights.
    pub mask: Vec<bool>,
}

pub struct World {
    pub level: Level,
    pub actors: SlotMap<ActorId, Actor>,
    pub hero: ActorId,
    /// Tiles the hero can see right now.
    pub visible: Vec<bool>,
    /// Tiles the hero has ever seen.
    pub seen: Vec<bool>,
    /// The hero's line-of-sight (also the mask for the hero's own light).
    pub hero_fov: Vec<bool>,
    pub torches: Vec<Torch>,
    pub rng: Rng,
    pub tick: u64,
    pub time: f32,
    fov_tile: Option<(i32, i32)>,
    fov_dirty: bool,
}

impl World {
    pub fn new(level: Level, seed: u64) -> World {
        let n = level.map.tiles.len();
        let mut actors = SlotMap::with_key();
        let hero = actors.insert(Actor::hero(level.start));
        let torches = level
            .torches()
            .into_iter()
            .map(|t| {
                let mask = fov::visible_from(&level.map, t.x, t.y, TORCH_RADIUS.ceil() as i32);
                Torch { tile: t, pos: Vec3::new(t.x as f32 + 0.5, 1.4, t.y as f32 + 0.5), mask }
            })
            .collect();
        let mut w = World {
            level,
            actors,
            hero,
            visible: vec![false; n],
            seen: vec![false; n],
            hero_fov: vec![false; n],
            torches,
            rng: Rng::new(seed, 7),
            tick: 0,
            time: 0.0,
            fov_tile: None,
            fov_dirty: true,
        };
        w.update_fov();
        w
    }

    pub fn map(&self) -> &TileMap {
        &self.level.map
    }

    pub fn hero(&self) -> &Actor {
        &self.actors[self.hero]
    }

    /// Recompute what the hero sees: line of sight AND lit by the hero's light or a torch.
    pub fn update_fov(&mut self) {
        let hp = self.actors[self.hero].pos;
        let (hi, hj) = self.level.map.tile_at(hp);
        if !self.fov_dirty && self.fov_tile == Some((hi, hj)) {
            return;
        }
        self.fov_dirty = false;
        self.fov_tile = Some((hi, hj));
        let map = &self.level.map;
        self.hero_fov = fov::visible_from(map, hi, hj, SIGHT_RADIUS);
        for j in 0..map.h {
            for i in 0..map.w {
                let k = map.idx(i, j);
                if !self.hero_fov[k] {
                    self.visible[k] = false;
                    continue;
                }
                let c = Vec2::new(i as f32 + 0.5, j as f32 + 0.5);
                let near_hero = c.distance(hp) <= HERO_LIGHT_RADIUS + 0.5;
                let lit = near_hero
                    || self.torches.iter().any(|t| t.mask[k] && Vec2::new(t.pos.x, t.pos.z).distance(c) <= TORCH_RADIUS);
                self.visible[k] = lit;
                if lit {
                    self.seen[k] = true;
                }
            }
        }
    }

    /// Mark the FOV for recomputation (e.g. a door opened).
    pub fn invalidate_fov(&mut self) {
        self.fov_dirty = true;
    }

    pub fn step(&mut self, input: &TickInput, dt: f32) {
        self.tick += 1;
        self.time += dt;
        let map = &self.level.map;
        let hero = &mut self.actors[self.hero];
        let mut dir = input.move_dir;
        if dir.length_squared() > 0.0 {
            hero.move_target = None;
        } else if let Some(t) = input.move_to {
            hero.move_target = Some(t);
        }
        if dir.length_squared() == 0.0 {
            if let Some(t) = hero.move_target {
                let d = t - hero.pos;
                if d.length() < 0.1 {
                    hero.move_target = None;
                } else {
                    dir = d.normalize();
                }
            }
        }
        let speed = hero.speed;
        if dir.length_squared() > 0.0 {
            hero.facing = dir.y.atan2(dir.x);
            let new = hero.pos + dir * speed * dt;
            hero.pos = collide_circle(map, new, hero.radius, |t| !t.walkable());
            hero.moving = true;
        } else {
            hero.moving = false;
        }
        self.update_fov();
    }
}
