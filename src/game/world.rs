//! The simulation state of one run and its fixed-timestep update.

use glam::{IVec2, Vec2, Vec3};
use slotmap::SlotMap;

use super::actor::{Actor, ActorId, Faction};
use super::defs::{AbilityDef, AreaDef, AreaFx, Defs, StatusApply};
use super::fov;
use super::level::Level;
use super::path::FlowField;
use super::rng::Rng;
use super::tile::TileMap;
use super::{ability, ai, combat, hero, projectile, room_flow};

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
    /// Enemy under the mouse.
    pub target: Option<ActorId>,
    /// Actions: 0 = basic (LMB), 1 = special (RMB), 2 = skill (Q), 3 = mobility (Space).
    pub act_pressed: [bool; 4],
    pub act_held: [bool; 4],
    pub act_released: [bool; 4],
    /// LMB pressed/held on open ground (move) rather than on an enemy.
    pub lmb_ground: bool,
    /// Reward choice (index into the offered upgrades).
    pub pick: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    Area(AreaFx),
    Hit,
    Blink,
    Spawn,
    Pickup,
    Heal,
    Dust,
    Bones,
    Splat,
}

/// Things that happened this tick, for sound, particles and screen shake.
#[derive(Clone, Debug)]
pub enum GameEvent {
    /// A melee swing (for the slash trail).
    Slash { pos: Vec2, dir: Vec2, range: f32, arc: f32, hero: bool },
    Damage { pos: Vec3, amount: f32, crit: bool, to_hero: bool, blocked: bool },
    Sfx(String),
    Died { pos: Vec2, hero: bool, boss: bool },
    Fx { kind: FxKind, pos: Vec2, radius: f32 },
    Shake(f32),
    Message(String),
    DoorsSealed,
    DoorsOpened,
    Victory,
    Defeat,
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub owner: Option<ActorId>,
    pub faction: Faction,
    pub figure: String,
    pub pos: Vec2,
    pub y: f32,
    pub vel: Vec2,
    pub range_left: f32,
    pub radius: f32,
    pub dmg: f32,
    pub pierce_left: u32,
    pub knockback: f32,
    pub on_hit: Vec<StatusApply>,
    pub explode: Option<AreaDef>,
    pub hit: Vec<ActorId>,
    pub alive: bool,
    pub age: f32,
}

#[derive(Clone, Debug)]
pub struct AreaEffect {
    pub owner: Option<ActorId>,
    pub faction: Faction,
    pub pos: Vec2,
    pub def: AreaDef,
    /// Seconds until the first pulse (telegraph).
    pub delay_left: f32,
    pub pulses_left: u32,
    pub pulse_t: f32,
    pub linger_left: f32,
    pub linger_tick: f32,
    pub dmg_mult: f32,
    pub age: f32,
}

impl AreaEffect {
    pub fn active(&self) -> bool {
        self.pulses_left > 0 || self.linger_left > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickupKind {
    Health(f32),
    Essence(f32),
}

#[derive(Clone, Debug)]
pub struct Pickup {
    pub pos: Vec2,
    pub kind: PickupKind,
    pub age: f32,
    pub taken: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunState {
    Playing,
    /// Choosing an upgrade: indices into `defs.upgrades`.
    Reward { choices: Vec<usize> },
    Dead { t: f32 },
    Victory { t: f32 },
}

#[derive(Clone, Debug, Default)]
pub struct RunStats {
    pub kills: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    pub rooms_cleared: u32,
    pub time: f32,
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
    pub projectiles: Vec<Projectile>,
    pub areas: Vec<AreaEffect>,
    pub pickups: Vec<Pickup>,
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
    pub events: Vec<GameEvent>,
    pub state: RunState,
    pub stats: RunStats,
    pub flow: FlowField,
    pub god: bool,
    pub seed: u64,
    /// Room whose reward is being chosen / the room the hero is in.
    pub current_room: Option<usize>,
    fov_tile: Option<(i32, i32)>,
    fov_dirty: bool,
}

/// Look up the abilities an actor uses (id + def pairs).
pub fn ability_list(defs: &Defs, ids: &[String]) -> Vec<(String, AbilityDef)> {
    ids.iter().filter_map(|id| defs.abilities.get(id).map(|d| (id.clone(), d.clone()))).collect()
}

impl World {
    pub fn new(level: Level, defs: &Defs, class: &str, seed: u64) -> World {
        let n = level.map.tiles.len();
        let mut actors = SlotMap::with_key();
        let cdef = defs.classes.get(class).or_else(|| defs.classes.values().next()).expect("no classes defined");
        let class_id = if defs.classes.contains_key(class) { class.to_string() } else { defs.classes.keys().next().cloned().unwrap_or_default() };
        let hero = actors.insert(Actor::hero(&class_id, cdef, &ability_list(defs, &cdef.abilities), level.start));
        let torches = level
            .torches()
            .into_iter()
            .map(|t| {
                let mask = fov::visible_from(&level.map, t.x, t.y, TORCH_RADIUS.ceil() as i32);
                Torch { tile: t, pos: Vec3::new(t.x as f32 + 0.5, 1.4, t.y as f32 + 0.5), mask }
            })
            .collect();
        let flow = FlowField::default();
        let mut w = World {
            level,
            actors,
            hero,
            projectiles: Vec::new(),
            areas: Vec::new(),
            pickups: Vec::new(),
            visible: vec![false; n],
            seen: vec![false; n],
            hero_fov: vec![false; n],
            torches,
            rng: Rng::new(seed, 7),
            tick: 0,
            time: 0.0,
            events: Vec::new(),
            state: RunState::Playing,
            stats: RunStats::default(),
            flow,
            god: false,
            seed,
            current_room: None,
            fov_tile: None,
            fov_dirty: true,
        };
        w.update_fov();
        w.rebuild_flow();
        w
    }

    pub fn map(&self) -> &TileMap {
        &self.level.map
    }

    pub fn hero(&self) -> &Actor {
        &self.actors[self.hero]
    }

    pub fn hero_mut(&mut self) -> &mut Actor {
        &mut self.actors[self.hero]
    }

    pub fn emit(&mut self, e: GameEvent) {
        self.events.push(e);
    }

    pub fn sfx(&mut self, s: &str) {
        if !s.is_empty() {
            self.events.push(GameEvent::Sfx(s.to_string()));
        }
    }

    pub fn tile_visible(&self, p: Vec2) -> bool {
        let (i, j) = self.level.map.tile_at(p);
        self.level.map.in_bounds(i, j) && self.visible[self.level.map.idx(i, j)]
    }

    /// Living enemies (ids), in a stable order.
    pub fn enemy_ids(&self) -> Vec<ActorId> {
        self.actors.iter().filter(|(_, a)| a.faction == Faction::Enemy && !a.dead).map(|(id, _)| id).collect()
    }

    /// Spawn an enemy by definition name.
    pub fn spawn_enemy(&mut self, defs: &Defs, id: &str, pos: Vec2, rise: bool, room: Option<usize>) -> Option<ActorId> {
        let e = defs.enemies.get(id)?;
        let depth_mult = 1.0 + 0.08 * room.map(|r| self.level.rooms[r].depth as f32).unwrap_or(0.0);
        let mut a = Actor::enemy(id, e, &ability_list(defs, &e.abilities), pos, depth_mult);
        a.room = room;
        let hp = self.hero().pos;
        a.face_towards(hp);
        a.ai.orbit_dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        // Stagger first attacks so groups don't swing in unison.
        for s in &mut a.slots {
            s.cd = self.rng.range_f(0.2, 1.2);
        }
        if rise {
            a.spawning = 0.8;
            self.emit(GameEvent::Fx { kind: FxKind::Spawn, pos, radius: a.radius });
        }
        Some(self.actors.insert(a))
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
        self.rebuild_flow();
    }

    pub fn rebuild_flow(&mut self) {
        let hp = self.actors[self.hero].pos;
        let t = self.level.map.tile_at(hp);
        self.flow = FlowField::build(&self.level.map, t, 400);
    }

    /// Mark the FOV for recomputation (e.g. a door opened).
    pub fn invalidate_fov(&mut self) {
        self.fov_dirty = true;
    }

    pub fn step(&mut self, defs: &Defs, input: &TickInput, dt: f32) {
        self.tick += 1;
        match self.state.clone() {
            RunState::Reward { choices } => {
                if let Some(k) = input.pick
                    && let Some(&ui) = choices.get(k) {
                        let u = defs.upgrades[ui].clone();
                        let class = self.hero().class_id().unwrap_or("").to_string();
                        if let Some(c) = defs.classes.get(&class) {
                            let hid = self.hero;
                            super::upgrade::take_upgrade(&mut self.actors[hid], &u, c, defs);
                        }
                        self.sfx("Upgrade");
                        self.emit(GameEvent::Message(format!("{} — {}", u.name, u.desc)));
                        self.state = RunState::Playing;
                    }
                return;
            }
            RunState::Dead { t } => {
                self.state = RunState::Dead { t: t + dt };
                self.time += dt;
                combat::update_deaths(self, defs, dt);
                return;
            }
            RunState::Victory { t } => {
                self.state = RunState::Victory { t: t + dt };
                self.time += dt;
                return;
            }
            RunState::Playing => {}
        }
        self.time += dt;
        self.stats.time += dt;

        hero::control(self, defs, input, dt);
        ai::think_all(self, defs, dt);
        ability::update_casts(self, defs, input, dt);
        ability::update_movement(self, defs, dt);
        projectile::update_projectiles(self, defs, dt);
        projectile::update_areas(self, defs, dt);
        combat::update_statuses(self, defs, dt);
        combat::update_deaths(self, defs, dt);
        projectile::update_pickups(self, dt);
        room_flow::update(self, defs, dt);
        self.update_fov();
        if self.flow.origin != self.level.map.tile_at(self.hero().pos) {
            self.rebuild_flow();
        }
        // Clean up.
        self.projectiles.retain(|p| p.alive);
        self.areas.retain(|a| a.delay_left > 0.0 || a.active());
        self.pickups.retain(|p| !p.taken);
        let dead_gone: Vec<ActorId> = self
            .actors
            .iter()
            .filter(|(id, a)| *id != self.hero && a.dead && a.death_t > 1.2)
            .map(|(id, _)| id)
            .collect();
        for id in dead_gone {
            self.actors.remove(id);
        }
    }

    /// A stable hash of the gameplay state (for determinism tests).
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x0100_0000_01b3);
        };
        for (_, a) in &self.actors {
            mix(a.pos.x.to_bits() as u64);
            mix(a.pos.y.to_bits() as u64);
            mix(a.hp.to_bits() as u64);
        }
        mix(self.projectiles.len() as u64);
        mix(self.areas.len() as u64);
        mix(self.rng.clone().next_u32() as u64);
        h
    }
}
