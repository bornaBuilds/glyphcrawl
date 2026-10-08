//! Enemy behaviour. Each enemy sets its `walk` velocity and starts casts; movement and casting
//! are then resolved by `ability`.

use glam::Vec2;

use super::ability::try_cast;
use super::actor::{ActorId, AiMode};
use super::defs::{AiKind, Defs, Effect, StatusApply, StatusKind};
use super::path::walk_line_clear;
use super::tile::Tile;
use super::world::{GameEvent, World};

/// Melee reach of an ability (its first MeleeArc / area range), if any.
fn reach(def: &super::defs::AbilityDef) -> Option<f32> {
    def.effects.iter().find_map(|e| match e {
        Effect::MeleeArc { range, .. } => Some(*range),
        Effect::AreaAtAim { max_range, area } => Some(max_range + area.radius * 0.5),
        Effect::Leap { dist, .. } => Some(*dist),
        Effect::Dash { dist, .. } => Some(*dist),
        _ => None,
    })
}

pub fn think_all(w: &mut World, defs: &Defs, dt: f32) {
    w.tick_hero_timers(dt);
    let hero = w.hero;
    let (hpos, hero_alive, hero_hidden) = {
        let h = &w.actors[hero];
        (h.pos, !h.dead, h.has(StatusKind::Invisible))
    };
    let ids: Vec<ActorId> = w.enemy_ids();
    for id in ids {
        let Some(a) = w.actors.get_mut(id) else { continue };
        a.walk = Vec2::ZERO;
        if a.spawning > 0.0 || a.disabled() || a.leap.is_some() || a.dash.is_some() {
            continue;
        }
        a.ai.lost_target = (a.ai.lost_target - dt).max(0.0);
        a.ai.timer -= dt;
        a.ai.think -= dt;
        let Some(eid) = a.enemy_id().map(str::to_string) else { continue };
        let Some(edef) = defs.enemies.get(&eid) else { continue };
        if !hero_alive || hero_hidden || a.ai.lost_target > 0.0 {
            // Wander a little while the hero is gone.
            if a.ai.think <= 0.0 {
                a.ai.think = 1.5;
                a.ai.orbit_dir = -a.ai.orbit_dir;
            }
            let wander = Vec2::new(a.ai.orbit_dir, (a.pos.x * 1.7).sin()).normalize_or_zero();
            a.walk = wander * a.move_speed() * 0.3;
            continue;
        }
        if a.cast.is_some() {
            continue;
        }
        match edef.ai {
            AiKind::Melee => melee(w, id, hpos),
            AiKind::Ranged { min, max } => ranged(w, id, hpos, min, max),
            AiKind::Slime => slime(w, id, hpos),
            AiKind::Bat => bat(w, id, hpos, dt),
            AiKind::Golem => golem(w, id, hpos),
        }
    }
}

/// Head towards `goal`: straight if the way is clear, otherwise follow the flow field.
fn chase_dir(w: &World, id: ActorId, goal: Vec2) -> Vec2 {
    let a = &w.actors[id];
    if a.flying || walk_line_clear(&w.level.map, a.pos, goal) {
        (goal - a.pos).normalize_or_zero()
    } else {
        w.flow.direction(&w.level.map, a.pos, a.flying).unwrap_or_else(|| (goal - a.pos).normalize_or_zero())
    }
}

fn melee(w: &mut World, id: ActorId, hpos: Vec2) {
    let a = &w.actors[id];
    let r = a.slots.first().and_then(|s| reach(&s.def)).unwrap_or(1.2);
    let d = a.pos.distance(hpos);
    let hero_r = w.actors[w.hero].radius;
    if d <= r * 0.85 + hero_r && a.slots[0].cd <= 0.0 {
        try_cast(w, id, 0, hpos, Some(w.hero));
        return;
    }
    if d > r * 0.6 + hero_r {
        let dir = chase_dir(w, id, hpos);
        let a = &mut w.actors[id];
        a.walk = dir * a.move_speed();
    } else {
        w.actors[id].face_towards(hpos);
    }
}

fn ranged(w: &mut World, id: ActorId, hpos: Vec2, min: f32, max: f32) {
    let a = &w.actors[id];
    let d = a.pos.distance(hpos);
    let los = w.level.map.line_clear(a.pos, hpos, |t: Tile| t.blocks_projectiles());
    let speed = a.move_speed();
    if los && d <= max + 1.0 && a.slots[0].cd <= 0.0 {
        try_cast(w, id, 0, hpos, Some(w.hero));
        return;
    }
    let a = &w.actors[id];
    let v = if d < min {
        // Back off, sliding along walls.
        let away = (a.pos - hpos).normalize_or_zero();
        let side = Vec2::new(-away.y, away.x) * a.ai.orbit_dir;
        let probe = a.pos + away * 0.8;
        if w.level.map.walkable_at(probe) { away } else { side }
    } else if d > max || !los {
        chase_dir(w, id, hpos)
    } else {
        // Strafe.
        let to = (hpos - a.pos).normalize_or_zero();
        Vec2::new(-to.y, to.x) * a.ai.orbit_dir * 0.6
    };
    let a = &mut w.actors[id];
    if a.ai.think <= 0.0 {
        a.ai.think = 1.2 + (a.pos.x.fract() * 1.3);
        a.ai.orbit_dir = -a.ai.orbit_dir;
    }
    a.walk = v * speed;
    a.face_towards(hpos);
}

fn slime(w: &mut World, id: ActorId, hpos: Vec2) {
    let a = &w.actors[id];
    let d = a.pos.distance(hpos);
    if a.slots[0].cd <= 0.0 && d < 7.0 {
        // Hop towards the hero (a bit short so it lands next to them).
        let aim = if walk_line_clear(&w.level.map, a.pos, hpos) {
            hpos
        } else {
            a.pos + chase_dir(w, id, hpos) * 2.0
        };
        try_cast(w, id, 0, aim, Some(w.hero));
    } else {
        let dir = chase_dir(w, id, hpos);
        let a = &mut w.actors[id];
        a.walk = dir * a.move_speed() * 0.25;
    }
}

fn bat(w: &mut World, id: ActorId, hpos: Vec2, dt: f32) {
    let a = &w.actors[id];
    let d = a.pos.distance(hpos);
    if a.ai.mode == AiMode::Retreat && a.ai.timer <= 0.0 {
        w.actors[id].ai.mode = AiMode::Orbit;
    }
    let a = &w.actors[id];
    if a.ai.mode != AiMode::Retreat && a.slots[0].cd <= 0.0 && d < 4.5
        && try_cast(w, id, 0, hpos, Some(w.hero)) {
            let a = &mut w.actors[id];
            a.ai.mode = AiMode::Retreat;
            a.ai.timer = 0.9;
            w.emit(GameEvent::Sfx("Bat".into()));
            return;
        }
    let a = &mut w.actors[id];
    let rel = a.pos - hpos;
    let ang = rel.y.atan2(rel.x) + a.ai.orbit_dir * dt * 1.6;
    let radius = if a.ai.mode == AiMode::Retreat { 4.0 } else { 2.6 };
    let goal = hpos + Vec2::new(ang.cos(), ang.sin()) * radius;
    let jitter = Vec2::new((w.time * 5.0 + a.pos.y).sin(), (w.time * 4.3 + a.pos.x).cos()) * 0.4;
    let dir = (goal - a.pos + jitter).normalize_or_zero();
    a.walk = dir * a.move_speed();
    if a.ai.think <= 0.0 {
        a.ai.think = 2.0 + (a.pos.x * 3.1).sin().abs() * 1.5;
        if (a.pos.y * 7.0).sin() > 0.3 {
            a.ai.orbit_dir = -a.ai.orbit_dir;
        }
    }
}

fn golem(w: &mut World, id: ActorId, hpos: Vec2) {
    let a = &w.actors[id];
    let frac = a.hp / a.stats.max_hp;
    let d = a.pos.distance(hpos);
    // Phase 2: call the dead once.
    if frac < 0.5 && !a.ai.summoned && a.slots.len() > 1
        && try_cast(w, id, 1, hpos, None) {
            let a = &mut w.actors[id];
            a.ai.summoned = true;
            w.emit(GameEvent::Message("The Bone Golem calls the dead!".into()));
            w.emit(GameEvent::Shake(0.3));
            return;
        }
    // Phase 3: enrage.
    if frac < 0.25 && !w.actors[id].ai.enraged {
        let a = &mut w.actors[id];
        a.ai.enraged = true;
        a.add_status(StatusApply { kind: StatusKind::Enraged, dur: 9999.0, mag: 0.0 });
        w.emit(GameEvent::Message("The Bone Golem is enraged!".into()));
        w.emit(GameEvent::Shake(0.4));
    }
    let a = &w.actors[id];
    let slam_r = a.slots.first().and_then(|s| reach(&s.def)).unwrap_or(2.5);
    if d <= slam_r && a.slots[0].cd <= 0.0 {
        try_cast(w, id, 0, hpos, Some(w.hero));
        return;
    }
    let los = w.level.map.line_clear(a.pos, hpos, |t: Tile| t.blocks_projectiles());
    if d > 4.5 && los && a.slots.len() > 2 && a.slots[2].cd <= 0.0 {
        try_cast(w, id, 2, hpos, Some(w.hero));
        return;
    }
    let dir = chase_dir(w, id, hpos);
    let a = &mut w.actors[id];
    a.walk = dir * a.move_speed();
}
