//! Room flow: entering a combat room seals the doors and starts waves; clearing it opens the
//! doors and offers an upgrade. The boss room ends the run.

use glam::{IVec2, Vec2};

use super::actor::Faction;
use super::defs::Defs;
use super::level::{RoomKind, RoomState};
use super::tile::Tile;
use super::upgrade::offer;
use super::world::{GameEvent, Pickup, PickupKind, RunState, World};

/// Seconds between one wave dying and the next arriving.
const WAVE_GAP: f32 = 1.2;

pub fn update(w: &mut World, defs: &Defs, dt: f32) {
    let hp = w.hero().pos;
    w.current_room = w.level.room_at(hp);
    for r in 0..w.level.rooms.len() {
        let room = &w.level.rooms[r];
        match room.state {
            RoomState::Dormant => {
                if !room.contains_interior(hp, 0.6) {
                    continue;
                }
                match room.kind {
                    RoomKind::Combat | RoomKind::Boss | RoomKind::Test => enter_fight(w, defs, r),
                    RoomKind::Treasure => {
                        w.level.rooms[r].state = RoomState::Cleared;
                        let c = w.level.rooms[r].center();
                        w.pickups.push(Pickup { pos: c, kind: PickupKind::Health(40.0), age: 0.0, taken: false });
                        give_reward(w, defs);
                        w.emit(GameEvent::Message("A forgotten shrine. Choose a blessing.".into()));
                    }
                    RoomKind::Start => w.level.rooms[r].state = RoomState::Cleared,
                }
            }
            RoomState::Active { wave } => {
                let boss_room = room.kind == RoomKind::Boss;
                let alive = w
                    .actors
                    .values()
                    .filter(|a| a.faction == Faction::Enemy && !a.dead && a.room == Some(r) && (!boss_room || a.boss))
                    .count();
                if alive > 0 {
                    continue;
                }
                if boss_room {
                    // The boss is down: its summons crumble.
                    let rest: Vec<_> = w.actors.iter().filter(|(_, a)| a.faction == Faction::Enemy && !a.dead).map(|(k, _)| k).collect();
                    for k in rest {
                        let a = &mut w.actors[k];
                        a.dead = true;
                        a.hp = 0.0;
                        a.start_action("die");
                    }
                }
                let room = &mut w.level.rooms[r];
                room.timer -= dt;
                if room.timer > 0.0 {
                    continue;
                }
                if wave + 1 < room.waves && room.kind != RoomKind::Boss {
                    room.state = RoomState::Active { wave: wave + 1 };
                    room.timer = WAVE_GAP;
                    spawn_wave(w, defs, r, wave + 1);
                } else {
                    clear_room(w, defs, r);
                }
            }
            RoomState::Cleared => {}
        }
    }
}

fn set_doors(w: &mut World, r: usize, t: Tile) {
    let doors: Vec<IVec2> = w.level.rooms[r].doors.clone();
    for d in doors {
        w.level.map.set(d.x, d.y, t);
    }
    w.invalidate_fov();
}

fn enter_fight(w: &mut World, defs: &Defs, r: usize) {
    if !w.level.rooms[r].doors.is_empty() {
        set_doors(w, r, Tile::DoorClosed);
        w.emit(GameEvent::DoorsSealed);
        w.sfx("DoorSlam");
        w.emit(GameEvent::Shake(0.25));
    }
    let room = &mut w.level.rooms[r];
    room.state = RoomState::Active { wave: 0 };
    room.timer = WAVE_GAP;
    if room.kind == RoomKind::Boss {
        let pos = room.boss.map(|b| Vec2::new(b.x as f32 + 0.5, b.y as f32 + 0.5)).unwrap_or(room.center());
        let boss = defs.enemies.iter().find(|(_, e)| e.boss).map(|(k, _)| k.clone());
        if let Some(b) = boss {
            w.spawn_enemy(defs, &b, pos, true, Some(r));
            w.emit(GameEvent::Message("The Bone Golem awakens.".into()));
        }
    } else {
        spawn_wave(w, defs, r, 0);
    }
}

/// Pick enemies for a wave by budget and spawn them at the room's spawn markers.
pub fn spawn_wave(w: &mut World, defs: &Defs, r: usize, wave: u32) {
    let room = w.level.rooms[r].clone();
    let depth = room.depth;
    let budget_total = 2.5 + depth as f32 * 1.1 + wave as f32 * 1.2;
    let candidates: Vec<(&String, &super::defs::EnemyDef)> = defs
        .enemies
        .iter()
        .filter(|(_, e)| !e.boss && !e.no_waves && (e.min_depth <= depth || room.kind == RoomKind::Test))
        .filter(|(k, _)| room.enemies.is_empty() || room.enemies.iter().any(|n| n == *k))
        .collect();
    if candidates.is_empty() {
        return;
    }
    let mut budget = budget_total;
    let mut picks: Vec<String> = Vec::new();
    let mut guard = 0;
    while budget > 0.5 && guard < 40 {
        guard += 1;
        let i = w.rng.below(candidates.len());
        let (k, e) = candidates[i];
        if e.cost <= budget + 0.25 {
            picks.push(k.clone());
            budget -= e.cost;
        }
    }
    let hp = w.hero().pos;
    let mut spots: Vec<Vec2> = room
        .spawns
        .iter()
        .map(|s| Vec2::new(s.x as f32 + 0.5, s.y as f32 + 0.5))
        .filter(|p| p.distance(hp) > 3.0)
        .collect();
    if spots.is_empty() {
        // Fall back to any floor in the room far from the hero.
        for j in room.origin.y + 1..room.origin.y + room.h - 1 {
            for i in room.origin.x + 1..room.origin.x + room.w - 1 {
                let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5);
                if w.level.map.walkable_at(p) && p.distance(hp) > 4.0 {
                    spots.push(p);
                }
            }
        }
    }
    if spots.is_empty() {
        return;
    }
    w.rng.shuffle(&mut spots);
    for (n, id) in picks.iter().enumerate() {
        let base = spots[n % spots.len()];
        let jitter = if n >= spots.len() { Vec2::new(w.rng.range_f(-0.35, 0.35), w.rng.range_f(-0.35, 0.35)) } else { Vec2::ZERO };
        let p = if w.level.map.walkable_at(base + jitter) { base + jitter } else { base };
        w.spawn_enemy(defs, id, p, true, Some(r));
    }
}

fn clear_room(w: &mut World, defs: &Defs, r: usize) {
    let kind = w.level.rooms[r].kind;
    w.level.rooms[r].state = RoomState::Cleared;
    w.stats.rooms_cleared += 1;
    if !w.level.rooms[r].doors.is_empty() {
        set_doors(w, r, Tile::DoorOpen);
        w.emit(GameEvent::DoorsOpened);
        w.sfx("DoorOpen");
    }
    if kind == RoomKind::Boss {
        w.state = RunState::Victory { t: 0.0 };
        w.sfx("Victory");
        w.emit(GameEvent::Victory);
        return;
    }
    w.emit(GameEvent::Message("Room cleared".into()));
    give_reward(w, defs);
}

fn give_reward(w: &mut World, defs: &Defs) {
    let hid = w.hero;
    let h = w.actors[hid].clone();
    let choices = offer(&mut w.rng, defs, &h);
    if !choices.is_empty() {
        w.state = RunState::Reward { choices };
    }
}
