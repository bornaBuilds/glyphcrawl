//! Seeded dungeon generator: a chain of hand-drawn room templates joined by corridors.
//!
//! start -> 4..6 combat rooms (with an optional shrine somewhere in the middle) -> boss arena.
//! Each new room is attached through a free door slot of the previous room: a straight
//! corridor leaves the door, and the new room is placed so one of its door slots (facing the
//! opposite way) meets the corridor's end. Rooms and corridors may not overlap.

use glam::{IVec2, Vec2};

use super::level::{Dir, Level, RoomInst, RoomKind, RoomState, RoomTemplate};
use super::rng::Rng;
use super::tile::{Tile, TileMap};

#[derive(Clone, Debug)]
struct Placed {
    template: usize,
    origin: IVec2,
    used_doors: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl Rect {
    fn grow(self, m: i32) -> Rect {
        Rect { x0: self.x0 - m, y0: self.y0 - m, x1: self.x1 + m, y1: self.y1 + m }
    }
    fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
    fn contains(&self, p: IVec2) -> bool {
        p.x >= self.x0 && p.x < self.x1 && p.y >= self.y0 && p.y < self.y1
    }
}

fn rect_of(t: &RoomTemplate, origin: IVec2) -> Rect {
    Rect { x0: origin.x, y0: origin.y, x1: origin.x + t.w, y1: origin.y + t.h }
}

/// The plan of room kinds along the critical path.
fn plan(rng: &mut Rng) -> Vec<RoomKind> {
    let combats = rng.range(4, 6) as usize;
    let mut v = vec![RoomKind::Start];
    let shrine_at = if rng.chance(0.75) { Some(rng.range(2, combats as i32 - 1) as usize) } else { None };
    for i in 0..combats {
        if Some(i) == shrine_at {
            v.push(RoomKind::Treasure);
        }
        v.push(RoomKind::Combat);
    }
    v.push(RoomKind::Boss);
    v
}

pub fn generate(templates: &[RoomTemplate], seed: u64) -> Result<Level, String> {
    for attempt in 0..40u64 {
        let mut rng = Rng::new(seed, 1000 + attempt);
        if let Some(level) = try_generate(templates, &mut rng, seed) {
            return Ok(level);
        }
    }
    Err(format!("could not generate a dungeon for seed {seed} (check the room templates' doors)"))
}

fn try_generate(templates: &[RoomTemplate], rng: &mut Rng, seed: u64) -> Option<Level> {
    let kinds = plan(rng);
    let of_kind = |k: RoomKind| -> Vec<usize> { (0..templates.len()).filter(|&i| templates[i].kind == k).collect() };
    let starts = of_kind(RoomKind::Start);
    let first = *rng.pick(&starts)?;
    let mut placed: Vec<Placed> = vec![Placed { template: first, origin: IVec2::ZERO, used_doors: Vec::new() }];
    let mut corridors: Vec<Vec<IVec2>> = Vec::new();
    let mut used_templates: Vec<usize> = vec![first];

    for &kind in &kinds[1..] {
        let mut candidates = of_kind(kind);
        if candidates.is_empty() {
            return None;
        }
        // Prefer templates not used yet, so runs show variety.
        let fresh: Vec<usize> = candidates.iter().copied().filter(|c| !used_templates.contains(c)).collect();
        if !fresh.is_empty() {
            candidates = fresh;
        }
        let mut ok = false;
        for _ in 0..80 {
            let prev = placed.last().unwrap().clone();
            let pt = &templates[prev.template];
            let free: Vec<usize> = (0..pt.doors.len()).filter(|d| !prev.used_doors.contains(d)).collect();
            let Some(&di) = rng.pick(&free) else { break };
            let (dpos, dir) = pt.doors[di];
            let a = prev.origin + dpos;
            let ti = *rng.pick(&candidates).unwrap();
            let t = &templates[ti];
            let matching: Vec<usize> = (0..t.doors.len()).filter(|&k| t.doors[k].1 == dir.opposite()).collect();
            let Some(&bi) = rng.pick(&matching) else { continue };
            let len = rng.range(2, 5);
            let d = dir.delta();
            let corridor: Vec<IVec2> = (1..=len).map(|k| a + d * k).collect();
            let b = a + d * (len + 1);
            let origin = b - t.doors[bi].0;
            let r = rect_of(t, origin);
            // No overlaps with other rooms (1 tile margin for walls) or corridors.
            let clash_room = placed.iter().any(|p| rect_of(&templates[p.template], p.origin).grow(1).overlaps(&r));
            let clash_corr = corridors.iter().flatten().any(|c| r.grow(1).contains(*c));
            let corr_hits_room = corridor.iter().any(|c| placed.iter().any(|p| rect_of(&templates[p.template], p.origin).grow(1).contains(*c)) && *c != a + d)
                || corridor.iter().any(|c| r.contains(*c));
            let corr_cross = corridor.iter().any(|c| corridors.iter().flatten().any(|o| (o.x - c.x).abs() <= 1 && (o.y - c.y).abs() <= 1));
            if clash_room || clash_corr || corr_hits_room || corr_cross {
                continue;
            }
            placed.last_mut().unwrap().used_doors.push(di);
            placed.push(Placed { template: ti, origin, used_doors: vec![bi] });
            corridors.push(corridor);
            used_templates.push(ti);
            ok = true;
            break;
        }
        if !ok {
            return None;
        }
    }

    // Rasterize.
    let mut lo = IVec2::splat(i32::MAX);
    let mut hi = IVec2::splat(i32::MIN);
    for p in &placed {
        let r = rect_of(&templates[p.template], p.origin);
        lo = lo.min(IVec2::new(r.x0, r.y0));
        hi = hi.max(IVec2::new(r.x1, r.y1));
    }
    let pad = 3;
    let shift = IVec2::splat(pad) - lo;
    let mut map = TileMap::new(hi.x - lo.x + pad * 2, hi.y - lo.y + pad * 2);
    let mut rooms = Vec::new();
    for (depth, p) in placed.iter().enumerate() {
        let t = &templates[p.template];
        let o = p.origin + shift;
        for j in 0..t.h {
            for i in 0..t.w {
                let tile = t.get(i, j);
                if tile != Tile::Void {
                    map.set(o.x + i, o.y + j, tile);
                }
            }
        }
        let doors: Vec<IVec2> = p.used_doors.iter().map(|&k| t.doors[k].0 + o).collect();
        for d in &doors {
            map.set(d.x, d.y, Tile::DoorOpen);
        }
        let combat_depth = placed[..=depth].iter().filter(|q| templates[q.template].kind == RoomKind::Combat).count() as u32;
        let kind = t.kind;
        rooms.push(RoomInst {
            template: p.template,
            kind,
            origin: o,
            w: t.w,
            h: t.h,
            doors,
            spawns: t.spawns.iter().map(|s| *s + o).collect(),
            chests: t.chests.iter().map(|s| *s + o).collect(),
            boss: t.boss.map(|b| b + o),
            waves: if kind == RoomKind::Combat { t.waves + u32::from(combat_depth >= 4) } else { t.waves },
            enemies: t.enemies.clone(),
            state: RoomState::Dormant,
            depth: combat_depth.max(1),
            timer: 0.0,
        });
    }
    for c in corridors.iter().flatten() {
        let p = *c + shift;
        map.set(p.x, p.y, Tile::Floor);
    }
    // Wall in the corridors.
    for c in corridors.iter().flatten() {
        let p = *c + shift;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if map.get(p.x + dx, p.y + dy) == Tile::Void {
                    map.set(p.x + dx, p.y + dy, Tile::Wall);
                }
            }
        }
    }
    let st = &templates[placed[0].template];
    let start_local = st.start.unwrap_or(IVec2::new(st.w / 2, st.h / 2));
    let s = start_local + placed[0].origin + shift;
    let _ = Dir::North;
    Some(Level { map, rooms, start: Vec2::new(s.x as f32 + 0.5, s.y as f32 + 0.5), seed })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn templates() -> Vec<RoomTemplate> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/rooms");
        let mut v = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&p).unwrap();
            v.push(RoomTemplate::parse(&name, &text).unwrap());
        }
        v
    }

    fn reachable(map: &TileMap, from: IVec2) -> Vec<bool> {
        let mut seen = vec![false; map.tiles.len()];
        let mut stack = vec![from];
        while let Some(p) = stack.pop() {
            if !map.in_bounds(p.x, p.y) || seen[map.idx(p.x, p.y)] || !map.get(p.x, p.y).walkable() {
                continue;
            }
            seen[map.idx(p.x, p.y)] = true;
            for d in [IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
                stack.push(p + d);
            }
        }
        seen
    }

    #[test]
    fn many_seeds_generate_valid_dungeons() {
        let t = templates();
        for seed in 0..300u64 {
            let lvl = generate(&t, seed).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(lvl.rooms.first().unwrap().kind, RoomKind::Start, "seed {seed}");
            assert_eq!(lvl.rooms.last().unwrap().kind, RoomKind::Boss, "seed {seed}");
            let combats = lvl.rooms.iter().filter(|r| r.kind == RoomKind::Combat).count();
            assert!((4..=6).contains(&combats), "seed {seed}: {combats} combat rooms");
            let s = IVec2::new(lvl.start.x as i32, lvl.start.y as i32);
            let reach = reachable(&lvl.map, s);
            for (k, r) in lvl.rooms.iter().enumerate() {
                // Some floor tile of every room is reachable from the start.
                let any = (r.origin.y + 1..r.origin.y + r.h - 1)
                    .flat_map(|j| (r.origin.x + 1..r.origin.x + r.w - 1).map(move |i| (i, j)))
                    .any(|(i, j)| reach[lvl.map.idx(i, j)]);
                assert!(any, "seed {seed}: room {k} unreachable");
                if r.kind == RoomKind::Combat {
                    assert!(r.spawns.len() >= 2, "seed {seed}: combat room {k} has <2 spawns");
                }
                for d in &r.doors {
                    let on_edge = d.x == r.origin.x || d.y == r.origin.y || d.x == r.origin.x + r.w - 1 || d.y == r.origin.y + r.h - 1;
                    assert!(on_edge, "seed {seed}: door not on room edge");
                }
            }
            // Rooms don't overlap.
            for (a, ra) in lvl.rooms.iter().enumerate() {
                for rb in lvl.rooms.iter().skip(a + 1) {
                    let x = ra.origin.x < rb.origin.x + rb.w && rb.origin.x < ra.origin.x + ra.w;
                    let y = ra.origin.y < rb.origin.y + rb.h && rb.origin.y < ra.origin.y + ra.h;
                    assert!(!(x && y), "seed {seed}: rooms overlap");
                }
            }
            assert!(lvl.map.w < 400 && lvl.map.h < 400);
        }
    }

    #[test]
    fn same_seed_same_dungeon() {
        let t = templates();
        let a = generate(&t, 77).unwrap();
        let b = generate(&t, 77).unwrap();
        assert_eq!(a.map, b.map);
        let c = generate(&t, 78).unwrap();
        assert_ne!(a.map, c.map);
    }
}
