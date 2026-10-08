//! Projectiles, area effects (telegraphed blasts, novas, burning ground) and pickups.


use super::ability::spawn_area;
use super::combat::{deal, Hit};
use super::defs::{AreaFx, Defs};
use super::world::{FxKind, GameEvent, PickupKind, World};

pub fn update_projectiles(w: &mut World, defs: &Defs, dt: f32) {
    let mut hits: Vec<Hit> = Vec::new();
    let mut explosions = Vec::new();
    for pi in 0..w.projectiles.len() {
        if !w.projectiles[pi].alive {
            continue;
        }
        let p = w.projectiles[pi].clone();
        let speed = p.vel.length();
        // Substeps so fast projectiles don't tunnel through enemies or walls.
        let steps = ((speed * dt) / 0.2).ceil().max(1.0) as i32;
        let mut pos = p.pos;
        let mut alive = true;
        let mut hit_list = p.hit.clone();
        let mut pierce = p.pierce_left;
        let mut range_left = p.range_left;
        for _ in 0..steps {
            let step = p.vel * dt / steps as f32;
            pos += step;
            range_left -= step.length();
            let (ti, tj) = w.level.map.tile_at(pos);
            if w.level.map.get(ti, tj).blocks_projectiles() {
                alive = false;
                if let Some(e) = &p.explode {
                    explosions.push((p.owner, p.faction, pos - step, e.clone(), p.dmg));
                }
                w.emit(GameEvent::Fx { kind: FxKind::Dust, pos: pos - step, radius: 0.2 });
                break;
            }
            let mut stop = false;
            for (oid, o) in w.actors.iter() {
                if o.faction == p.faction || !o.targetable() || hit_list.contains(&oid) {
                    continue;
                }
                // Projectiles fly at chest height: they pass over leaping slimes' shadows etc.
                if (o.y - 0.0).abs() > 1.6 && !o.flying {
                    continue;
                }
                if o.pos.distance(pos) < o.radius + p.radius {
                    hit_list.push(oid);
                    let mut h = Hit::new(oid, p.owner, p.dmg, pos - p.vel.normalize_or_zero());
                    h.knockback = p.knockback;
                    h.on_hit = p.on_hit.clone();
                    hits.push(h);
                    if let Some(e) = &p.explode {
                        explosions.push((p.owner, p.faction, pos, e.clone(), p.dmg));
                        stop = true;
                        break;
                    }
                    if pierce == 0 {
                        stop = true;
                        break;
                    }
                    pierce -= 1;
                }
            }
            if stop {
                alive = false;
                break;
            }
            if range_left <= 0.0 {
                alive = false;
                if let Some(e) = &p.explode {
                    explosions.push((p.owner, p.faction, pos, e.clone(), p.dmg));
                }
                break;
            }
        }
        let q = &mut w.projectiles[pi];
        q.pos = pos;
        q.alive = alive;
        q.hit = hit_list;
        q.pierce_left = pierce;
        q.range_left = range_left;
        q.age += dt;
    }
    for h in hits {
        deal(w, defs, h);
    }
    for (owner, faction, pos, area, _dmg) in explosions {
        spawn_area(w, owner, faction, pos, &area, 1.0);
    }
}

pub fn update_areas(w: &mut World, defs: &Defs, dt: f32) {
    let mut hits: Vec<Hit> = Vec::new();
    let mut fx: Vec<GameEvent> = Vec::new();
    for ai in 0..w.areas.len() {
        let a = &mut w.areas[ai];
        a.age += dt;
        if a.delay_left > 0.0 {
            a.delay_left -= dt;
            if a.delay_left > 0.0 {
                continue;
            }
        }
        let mut pulse = false;
        if a.pulses_left > 0 {
            a.pulse_t -= dt;
            if a.pulse_t <= 0.0 {
                pulse = true;
                a.pulses_left -= 1;
                a.pulse_t = a.def.interval.max(0.05);
                if a.pulses_left == 0 && a.def.linger > 0.0 {
                    a.linger_left = a.def.linger;
                }
            }
        } else if a.linger_left > 0.0 {
            a.linger_left -= dt;
            a.linger_tick -= dt;
        }
        let a = w.areas[ai].clone();
        if pulse {
            fx.push(GameEvent::Fx { kind: FxKind::Area(a.def.fx), pos: a.pos, radius: a.def.radius });
            let shake = match a.def.fx {
                AreaFx::Slam => 0.4,
                AreaFx::Fire | AreaFx::Blast => 0.15,
                _ => 0.0,
            };
            if shake > 0.0 {
                fx.push(GameEvent::Shake(shake));
            }
            let snd = match a.def.fx {
                AreaFx::Fire | AreaFx::Blast => "Explosion",
                AreaFx::Frost => "Frost",
                AreaFx::Slam => "Slam",
                AreaFx::Arrows => "Arrow",
                AreaFx::Splash => "Slime",
                _ => "",
            };
            if !snd.is_empty() {
                fx.push(GameEvent::Sfx(snd.into()));
            }
            for (oid, o) in w.actors.iter() {
                if o.faction == a.faction || !o.targetable() {
                    continue;
                }
                if o.pos.distance(a.pos) <= a.def.radius + o.radius * 0.5 {
                    let mut h = Hit::new(oid, a.owner, a.def.dmg * a.dmg_mult, a.pos);
                    h.knockback = a.def.knockback;
                    h.on_hit = a.def.on_hit.clone();
                    if a.def.dmg > 0.0 || !a.def.on_hit.is_empty() {
                        hits.push(h);
                    }
                }
            }
        } else if a.linger_left > 0.0 && a.linger_tick <= 0.0 {
            w.areas[ai].linger_tick = 0.5;
            for (oid, o) in w.actors.iter() {
                if o.faction == a.faction || !o.targetable() || o.flying {
                    continue;
                }
                if o.pos.distance(a.pos) <= a.def.radius {
                    let mut h = Hit::new(oid, a.owner, a.def.linger_dps * 0.5, a.pos);
                    h.dot = true;
                    hits.push(h);
                }
            }
        }
    }
    for e in fx {
        w.emit(e);
    }
    for h in hits {
        deal(w, defs, h);
    }
}

pub fn update_pickups(w: &mut World, dt: f32) {
    let hid = w.hero;
    let (hp, alive) = {
        let h = &w.actors[hid];
        (h.pos, !h.dead)
    };
    if !alive {
        return;
    }
    let mut taken = Vec::new();
    for (i, p) in w.pickups.iter_mut().enumerate() {
        p.age += dt;
        let d = hp.distance(p.pos);
        if d < 2.2 && p.age > 0.4 {
            // Magnet.
            let pull = (hp - p.pos).normalize_or_zero() * dt * (9.0 - d * 2.5).max(2.0);
            p.pos += pull;
        }
        if d < 0.45 && p.age > 0.4 {
            p.taken = true;
            taken.push(i);
        }
    }
    for i in taken {
        let kind = w.pickups[i].kind;
        let pos = w.pickups[i].pos;
        let h = &mut w.actors[hid];
        match kind {
            PickupKind::Health(v) => {
                h.hp = (h.hp + v).min(h.stats.max_hp);
                w.emit(GameEvent::Fx { kind: FxKind::Heal, pos, radius: 0.5 });
            }
            PickupKind::Essence(v) => {
                if let Some(r) = &mut h.resource {
                    r.cur = (r.cur + v).min(r.max);
                }
                w.emit(GameEvent::Fx { kind: FxKind::Pickup, pos, radius: 0.5 });
            }
        }
        w.sfx("Pickup");
    }
}
