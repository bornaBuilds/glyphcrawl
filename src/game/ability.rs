//! Casting abilities (wind-up -> fire -> recovery), the effects they produce, and movement
//! (walking, dashes, leaps, knockback, collisions).

use glam::Vec2;

use super::actor::{ActorId, Cast, CastPhase, DashState, Faction, LeapState};
use super::combat::{deal, Hit};
use super::defs::{AreaDef, Defs, Effect, StatusApply, StatusKind};
use super::tile::{collide_circle, Tile};
use super::world::{AreaEffect, FxKind, GameEvent, Projectile, TickInput, World};

/// Try to start casting ability `slot`. Returns false if it can't be cast right now.
pub fn try_cast(w: &mut World, id: ActorId, slot: usize, aim: Vec2, target: Option<ActorId>) -> bool {
    let Some(a) = w.actors.get(id) else { return false };
    if a.dead || a.disabled() || a.spawning > 0.0 || a.leap.is_some() {
        return false;
    }
    let Some(s) = a.slots.get(slot) else { return false };
    if s.cd > 0.0 {
        return false;
    }
    if let Some(r) = &a.resource {
        if r.cur + 1e-3 < s.def.cost {
            return false;
        }
    }
    // A cast in progress blocks new ones, except that recovery can be cancelled and mobility
    // skills (slot 3 for heroes) can interrupt anything but another dash.
    if let Some(c) = &a.cast {
        let mobility = a.is_hero() && slot == 3;
        let cancellable = c.phase == CastPhase::Recovery || (mobility && a.dash.is_none());
        if !cancellable {
            return false;
        }
    }
    if a.dash.is_some() {
        return false;
    }
    let def = s.def.clone();
    let a = &mut w.actors[id];
    if let Some(r) = &mut a.resource {
        r.cur -= def.cost;
    }
    a.remove_status(StatusKind::Blocking);
    // Hold/charge abilities start their cooldown on release.
    if !def.hold && def.charge <= 0.0 {
        a.slots[slot].cd = def.cooldown;
    }
    a.face_towards(aim);
    a.cast = Some(Cast { slot, phase: CastPhase::Windup, t: 0.0, aim, target, charge: 0.0 });
    a.move_target = None;
    a.path.clear();
    let wa = if def.windup_anim.is_empty() { &def.anim } else { &def.windup_anim };
    a.start_action(wa);
    true
}

/// Advance every actor's cast; fire effects when wind-ups finish.
pub fn update_casts(w: &mut World, defs: &Defs, input: &TickInput, dt: f32) {
    let ids: Vec<ActorId> = w.actors.keys().collect();
    for id in ids {
        let a = &w.actors[id];
        let Some(cast) = a.cast.clone() else { continue };
        if a.dead {
            continue;
        }
        if a.disabled() {
            let a = &mut w.actors[id];
            a.cast = None;
            a.remove_status(StatusKind::Blocking);
            continue;
        }
        let def = a.slots[cast.slot].def.clone();
        let is_hero = a.is_hero();
        let mut c = cast.clone();
        c.t += dt;
        match c.phase {
            CastPhase::Windup => {
                // Heroes keep aiming at the cursor while winding up.
                if is_hero {
                    c.aim = input.aim;
                    w.actors[id].face_towards(input.aim);
                }
                if c.t >= def.windup {
                    if def.hold || def.charge > 0.0 {
                        c.phase = CastPhase::Holding;
                        c.t = 0.0;
                        if def.effects.iter().any(|e| matches!(e, Effect::Block { .. })) {
                            w.actors[id].add_status(StatusApply { kind: StatusKind::Blocking, dur: 1.0, mag: 0.0 });
                        }
                        w.actors[id].cast = Some(c);
                    } else {
                        w.actors[id].cast = Some(Cast { phase: CastPhase::Recovery, t: 0.0, ..c.clone() });
                        let a = &mut w.actors[id];
                        a.start_action(&def.anim);
                        fire(w, defs, id, &def, c.aim, c.target, 1.0);
                    }
                } else {
                    w.actors[id].cast = Some(c);
                }
            }
            CastPhase::Holding => {
                let held = if is_hero { input.act_held[c.slot] } else { c.t < 0.6 };
                if is_hero {
                    c.aim = input.aim;
                    w.actors[id].face_towards(input.aim);
                }
                if def.charge > 0.0 {
                    c.charge = (c.charge + dt).min(def.charge);
                }
                if held {
                    w.actors[id].cast = Some(c);
                } else {
                    let a = &mut w.actors[id];
                    a.remove_status(StatusKind::Blocking);
                    a.slots[c.slot].cd = def.cooldown;
                    a.cast = Some(Cast { phase: CastPhase::Recovery, t: 0.0, ..c.clone() });
                    if def.charge > 0.0 {
                        a.start_action(&def.anim);
                        let k = c.charge / def.charge;
                        let mult = 1.0 + (def.charge_mult - 1.0) * k;
                        fire(w, defs, id, &def, c.aim, c.target, mult);
                    }
                }
            }
            CastPhase::Recovery => {
                if c.t >= def.recovery {
                    w.actors[id].cast = None;
                } else {
                    w.actors[id].cast = Some(c);
                }
            }
        }
    }
}

/// Clamp a target point to `max_range` from `from`.
fn clamp_range(from: Vec2, to: Vec2, max_range: f32) -> Vec2 {
    let d = to - from;
    if d.length() > max_range { from + d.normalize() * max_range } else { to }
}

pub fn spawn_area(w: &mut World, owner: Option<ActorId>, faction: Faction, pos: Vec2, def: &AreaDef, dmg_mult: f32) {
    w.areas.push(AreaEffect {
        owner,
        faction,
        pos,
        def: def.clone(),
        delay_left: def.delay,
        pulses_left: def.pulses.max(1),
        pulse_t: 0.0,
        linger_left: 0.0,
        linger_tick: 0.0,
        dmg_mult,
        age: 0.0,
    });
}

/// Fire all of an ability's effects.
pub fn fire(w: &mut World, defs: &Defs, id: ActorId, def: &super::defs::AbilityDef, aim: Vec2, target: Option<ActorId>, mult: f32) {
    let a = &w.actors[id];
    let (pos, faction, radius) = (a.pos, a.faction, a.radius);
    let dir = {
        let d = aim - pos;
        if d.length_squared() > 1e-4 { d.normalize() } else { a.facing_dir() }
    };
    w.sfx(&def.sfx);
    for e in &def.effects {
        match e {
            Effect::MeleeArc { range, arc, dmg, max_targets, knockback, backstab, on_hit } => {
                let cos_half = (arc.to_radians() / 2.0).cos();
                let mut cands: Vec<(f32, ActorId)> = w
                    .actors
                    .iter()
                    .filter(|(oid, o)| *oid != id && o.faction != faction && o.targetable())
                    .filter_map(|(oid, o)| {
                        let d = o.pos - pos;
                        let dist = d.length();
                        if dist > range + o.radius {
                            return None;
                        }
                        let ok = dist < o.radius + radius || d.normalize_or_zero().dot(dir) >= cos_half;
                        ok.then_some((dist, oid))
                    })
                    .collect();
                cands.sort_by(|x, y| x.0.total_cmp(&y.0));
                for (_, t) in cands.into_iter().take(*max_targets as usize) {
                    let mut h = Hit::new(t, Some(id), dmg * mult, pos);
                    h.knockback = *knockback;
                    h.backstab = *backstab;
                    h.on_hit = on_hit.clone();
                    deal(w, defs, h);
                }
                w.emit(GameEvent::Slash { pos, dir, range: *range, arc: *arc, hero: faction == Faction::Hero });
            }
            Effect::Projectile { figure, speed, range, radius: pr, dmg, count, spread, pierce, knockback, on_hit, explode, height } => {
                let n = (*count).max(1);
                for k in 0..n {
                    let ang = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * spread.to_radians() };
                    let (s, c) = ang.sin_cos();
                    let d = Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
                    w.projectiles.push(Projectile {
                        owner: Some(id),
                        faction,
                        figure: figure.clone(),
                        pos: pos + d * (radius + 0.15),
                        y: *height,
                        vel: d * *speed,
                        range_left: *range,
                        radius: *pr,
                        dmg: dmg * mult,
                        pierce_left: *pierce,
                        knockback: *knockback,
                        on_hit: on_hit.clone(),
                        explode: explode.clone(),
                        hit: Vec::new(),
                        alive: true,
                        age: 0.0,
                    });
                }
            }
            Effect::AreaAtAim { max_range, area } => {
                let mut p = clamp_range(pos, aim, *max_range);
                if let Some(t) = target.and_then(|t| w.actors.get(t)) {
                    if faction == Faction::Enemy {
                        p = clamp_range(pos, t.pos, *max_range);
                    }
                }
                spawn_area(w, Some(id), faction, p, area, mult);
            }
            Effect::AreaSelf { area } => spawn_area(w, Some(id), faction, pos, area, mult),
            Effect::Dash { dist, speed, iframes, pass_through, contact_dmg, knockback, on_hit, backwards } => {
                let a = &mut w.actors[id];
                // Heroes dash where they're steering (WASD) if they are, else at the cursor.
                let mut d = dir;
                if a.is_hero() && a.walk.length_squared() > 0.01 {
                    d = a.walk.normalize();
                }
                if *backwards {
                    d = -d;
                }
                a.dash = Some(DashState {
                    dir: d,
                    speed: *speed,
                    left: dist / speed,
                    contact_dmg: contact_dmg * mult,
                    knockback: *knockback,
                    pass_through: *pass_through,
                    iframes: *iframes,
                    on_hit: on_hit.clone(),
                    hit: Vec::new(),
                });
                if *iframes {
                    a.add_status(StatusApply { kind: StatusKind::IFrames, dur: dist / speed + 0.05, mag: 0.0 });
                }
                a.face_towards(pos + d);
                w.emit(GameEvent::Fx { kind: FxKind::Dust, pos, radius: 0.5 });
            }
            Effect::Blink { max_range } => {
                let goal = clamp_range(pos, aim, *max_range);
                let map = &w.level.map;
                let mut best = pos;
                let steps = 24;
                for k in (1..=steps).rev() {
                    let p = pos + (goal - pos) * (k as f32 / steps as f32);
                    let safe = collide_circle(map, p, radius, |t| !t.walkable());
                    if map.walkable_at(safe) && safe.distance(p) < 0.3 {
                        best = safe;
                        break;
                    }
                }
                w.emit(GameEvent::Fx { kind: FxKind::Blink, pos, radius: 0.6 });
                let a = &mut w.actors[id];
                a.pos = best;
                a.move_target = None;
                a.path.clear();
                w.emit(GameEvent::Fx { kind: FxKind::Blink, pos: best, radius: 0.6 });
            }
            Effect::Block { .. } => {}
            Effect::Buff { statuses } => {
                let a = &mut w.actors[id];
                for s in statuses {
                    a.add_status(*s);
                }
                if statuses.iter().any(|s| s.kind == StatusKind::Invisible) {
                    w.emit(GameEvent::Fx { kind: FxKind::Area(super::defs::AreaFx::Smoke), pos, radius: 1.8 });
                }
            }
            Effect::DropAggro => {
                let ids: Vec<ActorId> = w.actors.iter().filter(|(_, o)| o.faction != faction && !o.dead).map(|(k, _)| k).collect();
                for oid in ids {
                    let o = &mut w.actors[oid];
                    if o.cast.as_ref().is_some_and(|c| c.phase == super::actor::CastPhase::Windup) {
                        o.cast = None;
                    }
                    o.ai.lost_target = 2.5;
                }
            }
            Effect::Summon { enemy, count, radius: r } => {
                let room = w.actors[id].room;
                for k in 0..*count {
                    let ang = k as f32 / *count as f32 * std::f32::consts::TAU + w.rng.f32() * 0.5;
                    let p = pos + Vec2::new(ang.cos(), ang.sin()) * *r;
                    let p = if w.level.map.walkable_at(p) { p } else { pos };
                    w.spawn_enemy(defs, enemy, p, true, room);
                }
            }
            Effect::Leap { dist, height, dur, land } => {
                let to = clamp_range(pos, aim, *dist);
                let to = if w.level.map.walkable_at(to) { to } else { pos };
                let a = &mut w.actors[id];
                a.leap = Some(LeapState { from: pos, to, t: 0.0, dur: *dur, height: *height, land: land.clone() });
                a.anim.squash = 0.6;
            }
        }
    }
}

/// Walking, dashes, leaps, knockback, tile collisions and actor separation.
pub fn update_movement(w: &mut World, defs: &Defs, dt: f32) {
    let ids: Vec<ActorId> = w.actors.keys().collect();
    let mut contact_hits: Vec<Hit> = Vec::new();
    let mut landings: Vec<(ActorId, Vec2, AreaDef, Faction)> = Vec::new();
    for &id in &ids {
        let (pos, radius, faction) = {
            let a = &w.actors[id];
            (a.pos, a.radius, a.faction)
        };
        if w.actors[id].dead {
            continue;
        }
        let flying = w.actors[id].flying;
        let solid = move |t: Tile| if flying { !(t.walkable() || t == Tile::Pit) } else { !t.walkable() };

        // Dash.
        if let Some(mut d) = w.actors[id].dash.clone() {
            let step = d.dir * d.speed * dt;
            let new = collide_circle(&w.level.map, pos + step, radius, solid);
            let blocked = new.distance(pos + step) > 0.05;
            let mut stop = blocked;
            if d.contact_dmg > 0.0 || d.knockback > 0.0 || !d.pass_through {
                for (oid, o) in w.actors.iter() {
                    if oid == id || o.faction == faction || !o.targetable() || d.hit.contains(&oid) {
                        continue;
                    }
                    if o.pos.distance(new) < o.radius + radius + 0.1 {
                        d.hit.push(oid);
                        let mut h = Hit::new(oid, Some(id), d.contact_dmg, pos);
                        h.knockback = d.knockback;
                        h.on_hit = d.on_hit.clone();
                        contact_hits.push(h);
                        if !d.pass_through {
                            stop = true;
                        }
                    }
                }
            }
            d.left -= dt;
            let a = &mut w.actors[id];
            a.pos = new;
            a.moving = true;
            if d.left <= 0.0 || stop {
                a.dash = None;
                if d.iframes {
                    a.remove_status(StatusKind::IFrames);
                }
            } else {
                a.dash = Some(d);
            }
            continue;
        }

        // Leap.
        if let Some(mut l) = w.actors[id].leap.clone() {
            l.t += dt;
            let k = (l.t / l.dur).min(1.0);
            let a = &mut w.actors[id];
            a.pos = l.from.lerp(l.to, k);
            a.y = 4.0 * l.height * k * (1.0 - k);
            if k >= 1.0 {
                a.leap = None;
                a.y = 0.0;
                a.anim.squash = 0.55;
                if let Some(land) = l.land {
                    landings.push((id, a.pos, land, faction));
                }
            } else {
                a.leap = Some(l);
            }
            continue;
        }

        let a = &mut w.actors[id];
        let mut v = Vec2::ZERO;
        let casting_locked = a.cast.as_ref().is_some_and(|c| {
            let def = &a.slots[c.slot].def;
            match c.phase {
                super::actor::CastPhase::Windup => !def.mobile,
                super::actor::CastPhase::Holding => false,
                super::actor::CastPhase::Recovery => !a.is_hero(),
            }
        });
        if !a.disabled() && a.spawning <= 0.0 && !casting_locked {
            v = a.walk;
            if let Some(c) = &a.cast {
                if c.phase == super::actor::CastPhase::Holding {
                    let mm = a.slots[c.slot]
                        .def
                        .effects
                        .iter()
                        .find_map(|e| if let Effect::Block { move_mult, .. } = e { Some(*move_mult) } else { None })
                        .unwrap_or(0.5);
                    v *= mm;
                }
            }
        }
        a.moving = v.length_squared() > 0.01;
        if a.moving && a.cast.is_none() {
            a.facing = v.y.atan2(v.x);
        }
        let push = a.push;
        a.push *= (-9.0 * dt).exp();
        let target = pos + (v + push) * dt;
        a.pos = collide_circle(&w.level.map, target, radius, solid);
        if flying {
            a.y = 1.0 + 0.15 * (w.time * 3.0 + pos.x).sin();
        }
    }
    for h in contact_hits {
        deal(w, defs, h);
    }
    for (id, p, area, faction) in landings {
        spawn_area(w, Some(id), faction, p, &area, 1.0);
    }

    // Separation: living, grounded actors push each other apart.
    let bodies: Vec<(ActorId, Vec2, f32, bool)> = w
        .actors
        .iter()
        .filter(|(_, a)| !a.dead && a.leap.is_none() && a.dash.as_ref().is_none_or(|d| !d.pass_through))
        .map(|(id, a)| (id, a.pos, a.radius, a.is_hero() || a.boss))
        .collect();
    let mut shove: Vec<(ActorId, Vec2)> = Vec::new();
    for i in 0..bodies.len() {
        for j in i + 1..bodies.len() {
            let (ia, pa, ra, ha) = bodies[i];
            let (ib, pb, rb, hb) = bodies[j];
            let d = pb - pa;
            let min = ra + rb;
            let dist2 = d.length_squared();
            if dist2 < min * min && dist2 > 1e-8 {
                let dist = dist2.sqrt();
                let n = d / dist;
                let overlap = min - dist;
                // Heroes and bosses are heavy: others move out of their way.
                let (wa, wb) = match (ha, hb) {
                    (true, false) => (0.1, 0.9),
                    (false, true) => (0.9, 0.1),
                    _ => (0.5, 0.5),
                };
                shove.push((ia, -n * overlap * wa));
                shove.push((ib, n * overlap * wb));
            }
        }
    }
    for (id, s) in shove {
        let flying = w.actors[id].flying;
        let solid = move |t: Tile| if flying { !(t.walkable() || t == Tile::Pit) } else { !t.walkable() };
        let a = &mut w.actors[id];
        let r = a.radius;
        a.pos = collide_circle(&w.level.map, a.pos + s, r, solid);
    }
}
