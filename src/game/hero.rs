//! Turning the player's intent (`TickInput`) into hero actions.
//!
//! Diablo-style: left-click/hold on the ground walks there (A* path), on an enemy walks up and
//! attacks. With WASD held, left-click attacks towards the cursor instead (Hades-style).
//! Right-click / Q / Space cast the other three abilities at the cursor.

use glam::Vec2;

use super::ability::try_cast;
use super::actor::CastPhase;
use super::defs::Defs;
use super::path::{astar, walk_line_clear};
use super::world::{TickInput, World};

const BUFFER_TIME: f32 = 0.2;

pub fn control(w: &mut World, defs: &Defs, input: &TickInput, dt: f32) {
    let hid = w.hero;
    {
        let h = &mut w.actors[hid];
        h.walk = Vec2::ZERO;
        if h.dead {
            return;
        }
    }
    let class = w.actors[hid].class_id().unwrap_or("").to_string();
    let basic_range = defs.classes.get(&class).map(|c| c.basic_range).unwrap_or(1.5);

    // Buffered presses retry for a moment while another cast finishes.
    if let Some((slot, _aim, target, left)) = w.actors[hid].buffered {
        if try_cast(w, hid, slot, input.aim, target) || left - dt <= 0.0 {
            w.actors[hid].buffered = None;
        } else {
            w.actors[hid].buffered = Some((slot, input.aim, target, left - dt));
        }
    }

    // Right-click, Q, Space.
    for slot in 1..4 {
        if input.act_pressed[slot] && !try_cast(w, hid, slot, input.aim, input.target) {
            let h = &w.actors[hid];
            let ready = h.slots.get(slot).is_some_and(|s| s.cd <= 0.0);
            let affordable = h.slots.get(slot).is_some_and(|s| h.resource.as_ref().is_none_or(|r| r.cur + 1e-3 >= s.def.cost));
            if ready && affordable {
                w.actors[hid].buffered = Some((slot, input.aim, input.target, BUFFER_TIME));
            }
        }
    }

    let wasd = input.move_dir.length_squared() > 0.0;

    // Left mouse.
    let lmb = input.act_pressed[0] || input.act_held[0];
    if lmb {
        if let Some(t) = input.target {
            w.actors[hid].attack_target = Some(t);
            w.actors[hid].move_target = None;
            w.actors[hid].path.clear();
        } else if wasd || !input.lmb_ground {
            // Attack towards the cursor while steering with WASD.
            try_cast(w, hid, 0, input.aim, None);
        } else if input.act_pressed[0] || w.actors[hid].attack_target.is_none() {
            w.actors[hid].attack_target = None;
            set_move_target(w, input.aim, input.act_pressed[0]);
        }
    }

    // Attack-move.
    if let Some(t) = w.actors[hid].attack_target {
        let alive = w.actors.get(t).is_some_and(|a| a.targetable());
        if !alive {
            w.actors[hid].attack_target = None;
        } else {
            let (tpos, trad) = (w.actors[t].pos, w.actors[t].radius);
            let hpos = w.actors[hid].pos;
            let reach = basic_range + trad;
            // Ranged heroes also need a clear line of fire (no pillar in the way).
            let ranged = basic_range > 2.0;
            let clear = !ranged || w.level.map.line_clear(hpos, tpos, |t: super::tile::Tile| t.blocks_projectiles());
            if hpos.distance(tpos) <= reach && clear {
                if try_cast(w, hid, 0, tpos, Some(t)) && !input.act_held[0] {
                    // A single click attacks once.
                    w.actors[hid].attack_target = None;
                }
            } else if !wasd {
                let speed = w.actors[hid].move_speed();
                let dir = if walk_line_clear(&w.level.map, hpos, tpos) {
                    (tpos - hpos).normalize_or_zero()
                } else {
                    w.flow_towards_actor(hpos, tpos)
                };
                w.actors[hid].walk = dir * speed;
            }
        }
    }

    // WASD steering.
    let h = &mut w.actors[hid];
    if wasd {
        h.move_target = None;
        h.path.clear();
        h.attack_target = None;
        h.walk = input.move_dir.normalize() * h.move_speed();
        return;
    }
    // Follow the click-to-move path.
    if h.attack_target.is_none()
        && let Some(&next) = h.path.first() {
            let d = next - h.pos;
            let arrive = if h.path.len() == 1 { 0.08 } else { 0.35 };
            if d.length() < arrive {
                h.path.remove(0);
                if h.path.is_empty() {
                    h.move_target = None;
                }
            } else {
                let speed = h.move_speed();
                // Slow down at the very end so we don't overshoot.
                let s = if h.path.len() == 1 { speed.min(d.length() / dt) } else { speed };
                h.walk = d.normalize() * s;
            }
        }
    // Releasing the button finishes the current swing only.
    if !input.act_held[0]
        && let Some(c) = &h.cast
            && c.phase == CastPhase::Recovery && c.slot == 0 {
                h.attack_target = None;
            }
}

fn set_move_target(w: &mut World, goal: Vec2, fresh_click: bool) {
    let hid = w.hero;
    let h = &w.actors[hid];
    let goal_tile = w.level.map.tile_at(goal);
    let same = h.move_target.map(|t| w.level.map.tile_at(t)) == Some(goal_tile);
    if same && !fresh_click && h.path_t > 0.0 {
        return;
    }
    let from = h.pos;
    // Clicking a wall/pit walks to the nearest reachable spot towards it.
    let mut target = goal;
    if !w.level.map.walkable_at(goal) {
        let d = goal - from;
        let steps = (d.length() * 4.0).ceil() as i32;
        target = from;
        for k in (0..=steps).rev() {
            let p = from + d * (k as f32 / steps.max(1) as f32);
            if w.level.map.walkable_at(p) {
                target = p;
                break;
            }
        }
    }
    let path = astar(&w.level.map, from, target, 6000).unwrap_or_default();
    let h = &mut w.actors[hid];
    h.move_target = Some(target);
    h.path = path;
    h.path_t = 0.25;
}

impl World {
    /// Direction from `from` towards `to` using A* (for walking up to a target around walls).
    pub fn flow_towards_actor(&self, from: Vec2, to: Vec2) -> Vec2 {
        match astar(&self.level.map, from, to, 3000) {
            Some(p) if !p.is_empty() => (p[0] - from).normalize_or_zero(),
            _ => (to - from).normalize_or_zero(),
        }
    }

    pub fn tick_hero_timers(&mut self, dt: f32) {
        let hid = self.hero;
        let h = &mut self.actors[hid];
        h.path_t = (h.path_t - dt).max(0.0);
    }
}
