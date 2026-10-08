//! Damage, statuses, resources, deaths and drops.

use glam::{Vec2, Vec3};

use super::actor::{ActorId, Faction};
use super::defs::{Defs, StatusApply, StatusKind};
use super::world::{FxKind, GameEvent, Pickup, PickupKind, RunState, World};

/// Armour formula: 50 armour halves damage.
pub fn armor_factor(armor: f32) -> f32 {
    1.0 - armor.max(0.0) / (armor.max(0.0) + 50.0)
}

/// Is `attacker_pos` behind the target (outside a 120° cone in front of it)?
pub fn is_behind(target_facing: f32, target_pos: Vec2, attacker_pos: Vec2) -> bool {
    let to = (attacker_pos - target_pos).normalize_or_zero();
    let f = Vec2::new(target_facing.cos(), target_facing.sin());
    to.dot(f) < -0.5
}

/// Is `source` within the frontal `arc_deg` of an actor facing `facing`?
pub fn in_front(facing: f32, pos: Vec2, source: Vec2, arc_deg: f32) -> bool {
    let to = (source - pos).normalize_or_zero();
    let f = Vec2::new(facing.cos(), facing.sin());
    to.dot(f) >= (arc_deg.to_radians() / 2.0).cos()
}

pub struct Hit {
    pub target: ActorId,
    pub attacker: Option<ActorId>,
    pub dmg: f32,
    /// Where the blow came from (for blocking, knockback and backstabs).
    pub from: Vec2,
    pub knockback: f32,
    pub backstab: f32,
    pub on_hit: Vec<StatusApply>,
    /// Damage over time ticks don't crit, flash or build resources.
    pub dot: bool,
}

impl Hit {
    pub fn new(target: ActorId, attacker: Option<ActorId>, dmg: f32, from: Vec2) -> Hit {
        Hit { target, attacker, dmg, from, knockback: 0.0, backstab: 1.0, on_hit: Vec::new(), dot: false }
    }
}

/// Apply one hit. Returns the damage dealt.
pub fn deal(w: &mut World, defs: &Defs, h: Hit) -> f32 {
    let Some(t) = w.actors.get(h.target) else { return 0.0 };
    if !t.targetable() || t.has(StatusKind::IFrames) {
        return 0.0;
    }
    let target_is_hero = t.is_hero();
    let (tpos, tfacing, tarmor, ty) = (t.pos, t.facing, t.stats.armor, t.y);
    let mut dmg = h.dmg;
    let mut crit = false;
    if let Some(aid) = h.attacker
        && let Some(a) = w.actors.get_mut(aid) {
            dmg *= a.stats.dmg_mult;
            if !h.dot {
                let forced = a.has(StatusKind::NextHitCrit);
                if forced {
                    a.remove_status(StatusKind::NextHitCrit);
                    a.remove_status(StatusKind::Invisible);
                }
                let crit_roll = a.stats.crit;
                if forced || (crit_roll > 0.0 && w.rng.chance(crit_roll)) {
                    crit = true;
                    dmg *= 2.0;
                }
            }
        }
    if h.backstab > 1.0 && is_behind(tfacing, tpos, h.from) {
        dmg *= h.backstab;
        crit = true;
    }
    if !h.dot {
        dmg *= armor_factor(tarmor);
    }
    let mut blocked = false;
    {
        let t = &w.actors[h.target];
        if t.has(StatusKind::Blocking)
            && let Some(slot) = t.cast.as_ref().map(|c| c.slot)
                && let Some(super::defs::Effect::Block { arc, reduction, .. }) =
                    t.slots.get(slot).and_then(|s| s.def.effects.iter().find(|e| matches!(e, super::defs::Effect::Block { .. })))
                    && in_front(tfacing, tpos, h.from, *arc) {
                        dmg *= 1.0 - reduction.clamp(0.0, 1.0);
                        blocked = true;
                    }
    }
    if target_is_hero && w.god {
        dmg = 0.0;
    }
    let dmg = dmg.max(0.0);

    // Apply.
    let t = &mut w.actors[h.target];
    t.hp -= dmg;
    t.since_combat = 0.0;
    if !h.dot {
        t.anim.flash = 0.12;
        if !blocked && !t.boss && t.cast.is_none() {
            t.start_action("hurt");
        }
        if h.knockback > 0.0 && !blocked {
            let dir = (tpos - h.from).normalize_or_zero();
            t.push += dir * h.knockback * t.knockback_mult;
        }
        for s in &h.on_hit {
            t.add_status(*s);
        }
        // Being hurt builds resources (Fury).
        if let Some(r) = &mut t.resource {
            r.cur = (r.cur + r.def.on_hurt).min(r.max);
        }
    }
    let dead_now = t.hp <= 0.0 && !t.dead;
    if !h.dot
        && let Some(aid) = h.attacker
            && let Some(a) = w.actors.get_mut(aid) {
                a.since_combat = 0.0;
                if let Some(r) = &mut a.resource {
                    r.cur = (r.cur + r.def.on_hit).min(r.max);
                }
            }
    if target_is_hero {
        w.stats.damage_taken += dmg;
    } else {
        w.stats.damage_dealt += dmg;
    }
    w.emit(GameEvent::Damage { pos: Vec3::new(tpos.x, ty + 1.9, tpos.y), amount: dmg, crit, to_hero: target_is_hero, blocked });
    if !h.dot {
        w.sfx(if blocked {
            "Block"
        } else if crit {
            "Crit"
        } else if target_is_hero {
            "Hurt"
        } else {
            "Hit"
        });
        w.emit(GameEvent::Fx { kind: FxKind::Hit, pos: tpos, radius: 0.3 });
        if target_is_hero && dmg > 0.0 {
            w.emit(GameEvent::Shake(0.15));
        }
    }
    if dead_now {
        kill(w, defs, h.target);
    }
    dmg
}

fn kill(w: &mut World, _defs: &Defs, id: ActorId) {
    let a = &mut w.actors[id];
    a.dead = true;
    a.death_t = 0.0;
    a.hp = 0.0;
    a.cast = None;
    a.dash = None;
    a.statuses.clear();
    a.start_action("die");
    let (pos, hero, boss) = (a.pos, a.is_hero(), a.boss);
    w.emit(GameEvent::Died { pos, hero, boss });
    if hero {
        w.sfx("HeroDie");
        w.state = RunState::Dead { t: 0.0 };
        w.emit(GameEvent::Defeat);
        return;
    }
    w.stats.kills += 1;
    w.sfx("EnemyDie");
    w.emit(GameEvent::Fx { kind: FxKind::Bones, pos, radius: 0.5 });
    if boss {
        w.emit(GameEvent::Shake(0.6));
    }
}

/// Status timers, damage over time, resources, cooldowns, animation clocks.
pub fn update_statuses(w: &mut World, defs: &Defs, dt: f32) {
    let ids: Vec<ActorId> = w.actors.keys().collect();
    let mut dots: Vec<Hit> = Vec::new();
    for id in ids {
        let a = &mut w.actors[id];
        a.anim.flash = (a.anim.flash - dt).max(0.0);
        if let Some((_, t)) = &mut a.anim.action {
            *t += dt;
            if *t > 2.0 {
                a.anim.action = None;
            }
        }
        a.anim.squash += (1.0 - a.anim.squash) * (dt * 8.0).min(1.0);
        if a.dead {
            continue;
        }
        if a.spawning > 0.0 {
            a.spawning = (a.spawning - dt).max(0.0);
        }
        a.since_combat += dt;
        let cdr = a.stats.cdr + if a.has(StatusKind::Enraged) { 0.3 } else { 0.0 };
        for s in &mut a.slots {
            s.cd = (s.cd - dt * (1.0 + cdr)).max(0.0);
        }
        if let Some(r) = &mut a.resource {
            r.cur += r.def.regen * dt;
            if a.since_combat > r.def.decay_delay {
                r.cur -= r.def.decay * dt;
            }
            r.cur = r.cur.clamp(0.0, r.max);
        }
        for s in &mut a.statuses {
            s.left -= dt;
            if matches!(s.kind, StatusKind::Burning | StatusKind::Poison) && s.mag > 0.0 {
                // Tick twice a second.
                let before = ((s.left + dt) * 2.0).floor();
                let after = (s.left * 2.0).floor();
                if after < before {
                    let mut h = Hit::new(id, None, s.mag * 0.5, a.pos);
                    h.dot = true;
                    dots.push(h);
                }
            }
        }
        a.statuses.retain(|s| s.left > 0.0 || matches!(s.kind, StatusKind::Blocking | StatusKind::Enraged));
    }
    for h in dots {
        deal(w, defs, h);
    }
}

/// Death timers, drops and splitting.
pub fn update_deaths(w: &mut World, defs: &Defs, dt: f32) {
    let mut newly: Vec<ActorId> = Vec::new();
    for (id, a) in w.actors.iter_mut() {
        if a.dead {
            let before = a.death_t;
            a.death_t += dt;
            if before == 0.0 {
                newly.push(id);
            }
        }
    }
    for id in newly {
        let a = &w.actors[id];
        if a.faction != Faction::Enemy {
            continue;
        }
        let (pos, chance, split, room) = (a.pos, a.drop_chance, a.split.clone(), a.room);
        if w.rng.chance(chance) {
            let kind = if w.rng.chance(0.55) { PickupKind::Health(18.0) } else { PickupKind::Essence(30.0) };
            let off = Vec2::new(w.rng.range_f(-0.3, 0.3), w.rng.range_f(-0.3, 0.3));
            w.pickups.push(Pickup { pos: pos + off, kind, age: 0.0, taken: false });
        }
        if let Some((child, n)) = split {
            for k in 0..n {
                let ang = k as f32 / n as f32 * std::f32::consts::TAU + w.rng.f32();
                let p = pos + Vec2::new(ang.cos(), ang.sin()) * 0.45;
                let p = if w.level.map.walkable_at(p) { p } else { pos };
                if let Some(cid) = w.spawn_enemy(defs, &child, p, false, room) {
                    let c = &mut w.actors[cid];
                    c.push = Vec2::new(ang.cos(), ang.sin()) * 3.0;
                    c.anim.squash = 0.5;
                }
            }
            w.emit(GameEvent::Fx { kind: FxKind::Splat, pos, radius: 0.6 });
            w.sfx("Slime");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_halves_at_fifty() {
        assert!((armor_factor(50.0) - 0.5).abs() < 1e-6);
        assert!((armor_factor(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn behind_and_front() {
        // Target faces +x.
        assert!(is_behind(0.0, Vec2::ZERO, Vec2::new(-1.0, 0.0)));
        assert!(!is_behind(0.0, Vec2::ZERO, Vec2::new(1.0, 0.0)));
        assert!(in_front(0.0, Vec2::ZERO, Vec2::new(1.0, 0.3), 140.0));
        assert!(!in_front(0.0, Vec2::ZERO, Vec2::new(-1.0, 0.0), 140.0));
    }
}
