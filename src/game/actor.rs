//! Heroes and monsters.

use glam::Vec2;

use super::defs::{AbilityDef, AreaDef, ClassDef, EnemyDef, ResourceDef, StatusApply, StatusKind};

slotmap::new_key_type! {
    pub struct ActorId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Faction {
    Hero,
    Enemy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorKind {
    Hero { class: String },
    Enemy { def: String },
}

#[derive(Clone, Debug)]
pub struct Stats {
    pub max_hp: f32,
    pub armor: f32,
    pub speed: f32,
    pub dmg_mult: f32,
    pub crit: f32,
    /// Cooldown reduction 0..0.6.
    pub cdr: f32,
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub def: ResourceDef,
    pub cur: f32,
    pub max: f32,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub id: String,
    /// Effective definition (base + upgrades).
    pub def: AbilityDef,
    pub cd: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastPhase {
    Windup,
    /// Holding a held ability (block) or charging a charged one.
    Holding,
    Recovery,
}

#[derive(Clone, Debug)]
pub struct Cast {
    pub slot: usize,
    pub phase: CastPhase,
    pub t: f32,
    pub aim: Vec2,
    pub target: Option<super::actor::ActorId>,
    /// Seconds spent charging (charged abilities).
    pub charge: f32,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub kind: StatusKind,
    pub left: f32,
    pub mag: f32,
}

#[derive(Clone, Debug)]
pub struct DashState {
    pub dir: Vec2,
    pub speed: f32,
    pub left: f32,
    pub contact_dmg: f32,
    pub knockback: f32,
    pub pass_through: bool,
    pub iframes: bool,
    pub on_hit: Vec<StatusApply>,
    pub hit: Vec<ActorId>,
}

#[derive(Clone, Debug)]
pub struct LeapState {
    pub from: Vec2,
    pub to: Vec2,
    pub t: f32,
    pub dur: f32,
    pub height: f32,
    pub land: Option<AreaDef>,
}

/// Animation hints for the renderer (the sim never looks at these).
#[derive(Clone, Debug, Default)]
pub struct Anim {
    /// One-shot clip and how long it has been playing.
    pub action: Option<(String, f32)>,
    /// Seconds of white hit flash left.
    pub flash: f32,
    /// Squash & stretch (1 = none).
    pub squash: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AiMode {
    #[default]
    Chase,
    Retreat,
    Orbit,
    Recover,
}

#[derive(Clone, Debug, Default)]
pub struct AiState {
    pub mode: AiMode,
    pub timer: f32,
    /// Seconds until the next decision for erratic movers.
    pub think: f32,
    pub orbit_dir: f32,
    pub summoned: bool,
    pub enraged: bool,
    /// Seconds the enemy keeps ignoring an invisible hero.
    pub lost_target: f32,
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub kind: ActorKind,
    pub faction: Faction,
    pub name: String,
    /// Name of the 3D figure that draws this actor.
    pub figure: String,
    pub scale: f32,
    pub pos: Vec2,
    /// Height above the ground (bats fly, slimes hop, spawning enemies rise from below).
    pub y: f32,
    /// Knockback velocity (decays).
    pub push: Vec2,
    /// Facing angle on the ground plane (radians, atan2(z, x)).
    pub facing: f32,
    pub radius: f32,
    pub hp: f32,
    pub stats: Stats,
    pub resource: Option<Resource>,
    pub slots: Vec<Slot>,
    pub cast: Option<Cast>,
    pub dash: Option<DashState>,
    pub leap: Option<LeapState>,
    pub statuses: Vec<Status>,
    pub ai: AiState,
    pub anim: Anim,
    pub moving: bool,
    /// Desired walking velocity this tick (set by hero control / AI).
    pub walk: Vec2,
    /// Seconds until the hero's click-to-move path is recomputed.
    pub path_t: f32,
    pub move_target: Option<Vec2>,
    pub path: Vec<Vec2>,
    /// Hero: enemy to walk up to and hit.
    pub attack_target: Option<ActorId>,
    /// Hero: queued ability press (slot, aim, target, seconds left).
    pub buffered: Option<(usize, Vec2, Option<ActorId>, f32)>,
    pub dead: bool,
    pub death_t: f32,
    /// Seconds left rising out of the ground (inactive + invulnerable).
    pub spawning: f32,
    /// Seconds since this actor last dealt or took damage.
    pub since_combat: f32,
    pub room: Option<usize>,
    pub boss: bool,
    pub flying: bool,
    pub knockback_mult: f32,
    pub drop_chance: f32,
    pub split: Option<(String, u32)>,
    /// Upgrades taken (hero only): id -> stacks.
    pub upgrades: Vec<(String, u32)>,
}

impl Actor {
    fn base(kind: ActorKind, faction: Faction, name: String, figure: String, pos: Vec2) -> Actor {
        Actor {
            kind,
            faction,
            name,
            figure,
            scale: 1.0,
            pos,
            y: 0.0,
            push: Vec2::ZERO,
            facing: 0.8,
            radius: 0.32,
            hp: 1.0,
            stats: Stats { max_hp: 1.0, armor: 0.0, speed: 3.0, dmg_mult: 1.0, crit: 0.0, cdr: 0.0 },
            resource: None,
            slots: Vec::new(),
            cast: None,
            dash: None,
            leap: None,
            statuses: Vec::new(),
            ai: AiState::default(),
            anim: Anim { squash: 1.0, ..Default::default() },
            moving: false,
            walk: Vec2::ZERO,
            path_t: 0.0,
            move_target: None,
            path: Vec::new(),
            attack_target: None,
            buffered: None,
            dead: false,
            death_t: 0.0,
            spawning: 0.0,
            since_combat: 99.0,
            room: None,
            boss: false,
            flying: false,
            knockback_mult: 1.0,
            drop_chance: 0.0,
            split: None,
            upgrades: Vec::new(),
        }
    }

    pub fn hero(class_id: &str, c: &ClassDef, abilities: &[(String, AbilityDef)], pos: Vec2) -> Actor {
        let mut a = Actor::base(
            ActorKind::Hero { class: class_id.to_string() },
            Faction::Hero,
            c.name.clone(),
            c.figure.clone(),
            pos,
        );
        a.radius = c.radius;
        a.stats = Stats { max_hp: c.hp, armor: c.armor, speed: c.speed, dmg_mult: 1.0, crit: c.crit, cdr: 0.0 };
        a.hp = c.hp;
        a.resource = Some(Resource { def: c.resource.clone(), cur: c.resource.start, max: c.resource.max });
        a.slots = abilities.iter().map(|(id, d)| Slot { id: id.clone(), def: d.clone(), cd: 0.0 }).collect();
        a
    }

    pub fn enemy(id: &str, e: &EnemyDef, abilities: &[(String, AbilityDef)], pos: Vec2, hp_mult: f32) -> Actor {
        let mut a = Actor::base(ActorKind::Enemy { def: id.to_string() }, Faction::Enemy, e.name.clone(), e.figure.clone(), pos);
        a.radius = e.radius;
        a.scale = e.scale;
        a.stats = Stats { max_hp: e.hp * hp_mult, armor: e.armor, speed: e.speed, dmg_mult: 1.0, crit: 0.0, cdr: 0.0 };
        a.hp = a.stats.max_hp;
        a.slots = abilities.iter().map(|(id, d)| Slot { id: id.clone(), def: d.clone(), cd: 0.0 }).collect();
        a.boss = e.boss;
        a.flying = e.flying;
        a.knockback_mult = e.knockback_mult;
        a.drop_chance = e.drop_chance;
        a.split = e.split.clone();
        if a.flying {
            a.y = 1.0;
        }
        a
    }

    pub fn is_hero(&self) -> bool {
        self.faction == Faction::Hero
    }

    pub fn alive(&self) -> bool {
        !self.dead
    }

    /// Can be hit right now.
    pub fn targetable(&self) -> bool {
        !self.dead && self.spawning <= 0.0
    }

    pub fn has(&self, k: StatusKind) -> bool {
        self.statuses.iter().any(|s| s.kind == k)
    }

    pub fn status_mag(&self, k: StatusKind) -> f32 {
        self.statuses.iter().filter(|s| s.kind == k).map(|s| s.mag).fold(0.0, f32::max)
    }

    pub fn add_status(&mut self, s: StatusApply) {
        if self.boss && matches!(s.kind, StatusKind::Stun | StatusKind::Freeze) {
            // Bosses shrug off hard crowd control (but get slowed instead).
            self.add_status(StatusApply { kind: StatusKind::Slow, dur: s.dur, mag: 0.4 });
            return;
        }
        if let Some(e) = self.statuses.iter_mut().find(|e| e.kind == s.kind) {
            e.left = e.left.max(s.dur);
            e.mag = e.mag.max(s.mag);
        } else {
            self.statuses.push(Status { kind: s.kind, left: s.dur, mag: s.mag });
        }
    }

    pub fn remove_status(&mut self, k: StatusKind) {
        self.statuses.retain(|s| s.kind != k);
    }

    /// Stunned or frozen: no moving, no casting.
    pub fn disabled(&self) -> bool {
        self.has(StatusKind::Stun) || self.has(StatusKind::Freeze)
    }

    pub fn move_speed(&self) -> f32 {
        let mut s = self.stats.speed;
        if self.has(StatusKind::Slow) {
            s *= 1.0 - self.status_mag(StatusKind::Slow).clamp(0.0, 0.8);
        }
        if self.has(StatusKind::Haste) {
            s *= 1.0 + self.status_mag(StatusKind::Haste);
        }
        if self.has(StatusKind::Enraged) {
            s *= 1.35;
        }
        s
    }

    pub fn start_action(&mut self, clip: &str) {
        if !clip.is_empty() {
            self.anim.action = Some((clip.to_string(), 0.0));
        }
    }

    pub fn class_id(&self) -> Option<&str> {
        match &self.kind {
            ActorKind::Hero { class } => Some(class),
            _ => None,
        }
    }

    pub fn enemy_id(&self) -> Option<&str> {
        match &self.kind {
            ActorKind::Enemy { def } => Some(def),
            _ => None,
        }
    }

    pub fn facing_dir(&self) -> Vec2 {
        Vec2::new(self.facing.cos(), self.facing.sin())
    }

    pub fn face_towards(&mut self, p: Vec2) {
        let d = p - self.pos;
        if d.length_squared() > 1e-6 {
            self.facing = d.y.atan2(d.x);
        }
    }
}
