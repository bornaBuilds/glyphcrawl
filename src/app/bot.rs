//! A simple autoplayer used by headless runs to find panics and measure performance.
//! It fights the nearest enemy, uses skills when ready, and walks to the next room.

use crate::game::actor::Faction;
use crate::game::defs::Defs;
use crate::game::level::RoomState;
use crate::game::world::{RunState, TickInput, World};

pub fn think(w: &World, defs: &Defs) -> TickInput {
    let mut ti = TickInput::default();
    if let RunState::Reward { .. } = w.state {
        ti.pick = Some((w.tick % 3) as usize);
        return ti;
    }
    let h = w.hero();
    let _ = defs;
    let nearest = w
        .actors
        .iter()
        .filter(|(_, a)| a.faction == Faction::Enemy && a.targetable() && w.tile_visible(a.pos))
        .map(|(id, a)| (id, a.pos, a.pos.distance(h.pos)))
        .min_by(|x, y| x.2.total_cmp(&y.2));
    if let Some((id, pos, d)) = nearest {
        ti.aim = pos;
        ti.target = Some(id);
        ti.act_pressed[0] = true;
        ti.act_held[0] = true;
        // Skills on a rhythm so they don't all fire on the same tick.
        let beat = w.tick % 90;
        if d < 7.0 && beat == 10 {
            ti.act_pressed[1] = true;
            ti.act_held[1] = true;
        }
        if d < 3.5 && beat == 40 {
            ti.act_pressed[2] = true;
        }
        if h.hp < h.stats.max_hp * 0.4 && beat == 70 {
            // Get away from it.
            ti.aim = h.pos + (h.pos - pos).normalize_or_zero() * 4.0;
            ti.act_pressed[3] = true;
        }
        return ti;
    }
    // No enemies in sight: head for the first room that isn't cleared yet.
    let goal = w
        .level
        .rooms
        .iter()
        .find(|r| r.state != RoomState::Cleared)
        .map(|r| r.center())
        .unwrap_or(h.pos);
    // Aim a few tiles towards the goal so the hero's A* handles the details.
    ti.aim = goal;
    ti.lmb_ground = true;
    if w.tick.is_multiple_of(20) || h.path.is_empty() {
        ti.act_pressed[0] = true;
    }
    ti.act_held[0] = true;
    ti
}
