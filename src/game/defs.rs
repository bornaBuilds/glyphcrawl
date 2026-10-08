//! Game data loaded from RON: classes, abilities, enemies, upgrades.
//! Everything balance-related lives in `assets/*.ron`, not in code.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum ResourceKind {
    Fury,
    Mana,
    Energy,
    Focus,
}

impl ResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            ResourceKind::Fury => "FURY",
            ResourceKind::Mana => "MANA",
            ResourceKind::Energy => "ENERGY",
            ResourceKind::Focus => "FOCUS",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResourceDef {
    pub kind: ResourceKind,
    pub max: f32,
    #[serde(default)]
    pub start: f32,
    /// Per second, always.
    #[serde(default)]
    pub regen: f32,
    /// Per second, after `decay_delay` seconds out of combat.
    #[serde(default)]
    pub decay: f32,
    #[serde(default)]
    pub decay_delay: f32,
    /// Gained per enemy hit dealt.
    #[serde(default)]
    pub on_hit: f32,
    /// Gained when taking damage.
    #[serde(default)]
    pub on_hurt: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClassDef {
    pub name: String,
    pub figure: String,
    pub desc: String,
    pub color: String,
    pub hp: f32,
    pub armor: f32,
    pub speed: f32,
    #[serde(default = "default_radius")]
    pub radius: f32,
    #[serde(default)]
    pub crit: f32,
    pub resource: ResourceDef,
    /// Basic (LMB), special (RMB), skill (Q), mobility (Space).
    pub abilities: Vec<String>,
    /// Attack-move range for the basic attack (how close to walk before swinging).
    pub basic_range: f32,
}

fn default_radius() -> f32 {
    0.32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum StatusKind {
    Stun,
    Freeze,
    Slow,
    Invisible,
    IFrames,
    Burning,
    Poison,
    NextHitCrit,
    Blocking,
    Enraged,
    Haste,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct StatusApply {
    pub kind: StatusKind,
    pub dur: f32,
    #[serde(default)]
    pub mag: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
pub enum AreaFx {
    #[default]
    Blast,
    Fire,
    Frost,
    Arrows,
    Slam,
    Poison,
    Smoke,
    Splash,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AreaDef {
    pub radius: f32,
    /// Seconds before it goes off (shown as a telegraph).
    #[serde(default)]
    pub delay: f32,
    #[serde(default)]
    pub dmg: f32,
    #[serde(default = "one_u32")]
    pub pulses: u32,
    #[serde(default)]
    pub interval: f32,
    #[serde(default)]
    pub knockback: f32,
    #[serde(default)]
    pub on_hit: Vec<StatusApply>,
    #[serde(default)]
    pub fx: AreaFx,
    /// Burning ground etc.: keeps dealing `linger_dps` for this many seconds.
    #[serde(default)]
    pub linger: f32,
    #[serde(default)]
    pub linger_dps: f32,
}

fn one_u32() -> u32 {
    1
}

fn one_f32() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
pub enum Effect {
    MeleeArc {
        range: f32,
        arc: f32,
        dmg: f32,
        #[serde(default = "one_u32")]
        max_targets: u32,
        #[serde(default)]
        knockback: f32,
        /// Damage multiplier when hitting from behind.
        #[serde(default = "one_f32")]
        backstab: f32,
        #[serde(default)]
        on_hit: Vec<StatusApply>,
    },
    Projectile {
        figure: String,
        speed: f32,
        range: f32,
        #[serde(default = "proj_radius")]
        radius: f32,
        dmg: f32,
        #[serde(default = "one_u32")]
        count: u32,
        #[serde(default)]
        spread: f32,
        #[serde(default)]
        pierce: u32,
        #[serde(default)]
        knockback: f32,
        #[serde(default)]
        on_hit: Vec<StatusApply>,
        #[serde(default)]
        explode: Option<AreaDef>,
        /// Projectile flight height.
        #[serde(default = "proj_height")]
        height: f32,
    },
    /// Area centred on the cursor (clamped to `max_range`), e.g. Volley, Golem slam.
    AreaAtAim { max_range: f32, area: AreaDef },
    /// Area centred on the caster, e.g. Frost Nova.
    AreaSelf { area: AreaDef },
    Dash {
        dist: f32,
        speed: f32,
        #[serde(default)]
        iframes: bool,
        #[serde(default)]
        pass_through: bool,
        #[serde(default)]
        contact_dmg: f32,
        #[serde(default)]
        knockback: f32,
        #[serde(default)]
        on_hit: Vec<StatusApply>,
        /// Dash away from the aim point instead of towards it.
        #[serde(default)]
        backwards: bool,
    },
    Blink { max_range: f32 },
    Block { arc: f32, reduction: f32, move_mult: f32 },
    Buff { statuses: Vec<StatusApply> },
    /// Enemies lose track of the caster.
    DropAggro,
    Summon { enemy: String, count: u32, radius: f32 },
    /// Jump towards the aim point and land with an optional area.
    Leap { dist: f32, height: f32, dur: f32, #[serde(default)] land: Option<AreaDef> },
}

fn proj_radius() -> f32 {
    0.25
}

fn proj_height() -> f32 {
    0.9
}

#[derive(Clone, Debug, Deserialize)]
pub struct AbilityDef {
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub cost: f32,
    #[serde(default)]
    pub cooldown: f32,
    #[serde(default)]
    pub windup: f32,
    #[serde(default)]
    pub recovery: f32,
    /// Held abilities (Block) stay active while the button is held.
    #[serde(default)]
    pub hold: bool,
    /// Charged abilities: hold to charge up to this many seconds, release to fire.
    /// Damage scales from 1x to `charge_mult`.
    #[serde(default)]
    pub charge: f32,
    #[serde(default = "one_f32")]
    pub charge_mult: f32,
    #[serde(default)]
    pub anim: String,
    /// Animation for the wind-up (enemies telegraph with it).
    #[serde(default)]
    pub windup_anim: String,
    #[serde(default)]
    pub sfx: String,
    /// The caster can keep moving during the wind-up.
    #[serde(default)]
    pub mobile: bool,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum AiKind {
    Melee,
    Ranged { min: f32, max: f32 },
    Slime,
    Bat,
    Golem,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EnemyDef {
    pub name: String,
    pub figure: String,
    pub hp: f32,
    #[serde(default)]
    pub armor: f32,
    pub speed: f32,
    #[serde(default = "default_radius")]
    pub radius: f32,
    #[serde(default = "one_f32")]
    pub scale: f32,
    pub ai: AiKind,
    pub abilities: Vec<String>,
    /// Wave budget cost.
    #[serde(default = "one_f32")]
    pub cost: f32,
    /// Earliest room depth where it appears in waves.
    #[serde(default)]
    pub min_depth: u32,
    #[serde(default)]
    pub flying: bool,
    #[serde(default)]
    pub boss: bool,
    /// Spawn these on death (e.g. slime splitting).
    #[serde(default)]
    pub split: Option<(String, u32)>,
    #[serde(default)]
    pub drop_chance: f32,
    /// Knockback multiplier (0 = immovable).
    #[serde(default = "one_f32")]
    pub knockback_mult: f32,
    /// Excluded from random waves.
    #[serde(default)]
    pub no_waves: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum StatKind {
    MaxHp,
    Armor,
    Speed,
    Damage,
    Crit,
    Cdr,
    ResourceMax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum AbilityField {
    Damage,
    Cooldown,
    Cost,
    Radius,
    Range,
    Count,
    Pierce,
    Arc,
    MaxTargets,
    StatusDur,
    Linger,
    Pulses,
    Knockback,
    Spread,
}

#[derive(Clone, Debug, Deserialize)]
pub enum Modifier {
    /// `add` is added, then the total is multiplied by (1 + `mul`).
    Stat { stat: StatKind, #[serde(default)] add: f32, #[serde(default)] mul: f32 },
    Ability { slot: usize, field: AbilityField, #[serde(default)] add: f32, #[serde(default)] mul: f32 },
    AddOnHit { slot: usize, status: StatusApply },
    Heal(f32),
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpgradeDef {
    pub id: String,
    pub name: String,
    pub desc: String,
    #[serde(default)]
    pub class: Option<String>,
    pub mods: Vec<Modifier>,
    #[serde(default = "one_u32")]
    pub max: u32,
    #[serde(default = "weight")]
    pub weight: u32,
}

fn weight() -> u32 {
    10
}

/// All gameplay data.
#[derive(Clone, Debug, Default)]
pub struct Defs {
    pub classes: BTreeMap<String, ClassDef>,
    pub abilities: BTreeMap<String, AbilityDef>,
    pub enemies: BTreeMap<String, EnemyDef>,
    pub upgrades: Vec<UpgradeDef>,
}

impl Defs {
    /// Check cross-references (class -> abilities, enemies -> abilities/splits).
    pub fn validate(&self) -> Result<(), String> {
        for (id, c) in &self.classes {
            if c.abilities.len() != 4 {
                return Err(format!("class '{id}': needs exactly 4 abilities (LMB, RMB, Q, Space)"));
            }
            for a in &c.abilities {
                if !self.abilities.contains_key(a) {
                    return Err(format!("class '{id}': unknown ability '{a}'"));
                }
            }
        }
        for (id, e) in &self.enemies {
            for a in &e.abilities {
                if !self.abilities.contains_key(a) {
                    return Err(format!("enemy '{id}': unknown ability '{a}'"));
                }
            }
            if let Some((s, _)) = &e.split {
                if !self.enemies.contains_key(s) {
                    return Err(format!("enemy '{id}': unknown split enemy '{s}'"));
                }
            }
        }
        for (id, a) in &self.abilities {
            for e in &a.effects {
                if let Effect::Summon { enemy, .. } = e {
                    if !self.enemies.contains_key(enemy) {
                        return Err(format!("ability '{id}': unknown summoned enemy '{enemy}'"));
                    }
                }
            }
        }
        for u in &self.upgrades {
            if let Some(c) = &u.class {
                if !self.classes.contains_key(c) {
                    return Err(format!("upgrade '{}': unknown class '{c}'", u.id));
                }
            }
        }
        Ok(())
    }
}
