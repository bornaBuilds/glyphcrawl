//! Pathfinding: a flow field towards the hero (every enemy shares it) and A* for the hero's
//! click-to-move.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::Vec2;

use super::tile::{Tile, TileMap};

const NEIGHBOURS: [(i32, i32, u32); 8] =
    [(1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10), (1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14)];

fn passable(map: &TileMap, i: i32, j: i32, flying: bool) -> bool {
    let t = map.get(i, j);
    t.walkable() || (flying && t == Tile::Pit)
}

/// Diagonal moves may not cut wall corners.
fn step_ok(map: &TileMap, i: i32, j: i32, di: i32, dj: i32, flying: bool) -> bool {
    if !passable(map, i + di, j + dj, flying) {
        return false;
    }
    if di != 0 && dj != 0 {
        return passable(map, i + di, j, flying) && passable(map, i, j + dj, flying);
    }
    true
}

/// Distance (×10) from every tile to the target tile.
#[derive(Clone, Debug, Default)]
pub struct FlowField {
    pub w: i32,
    pub h: i32,
    pub dist: Vec<u32>,
    pub origin: (i32, i32),
}

impl FlowField {
    pub fn build(map: &TileMap, target: (i32, i32), max_cost: u32) -> FlowField {
        let mut dist = vec![u32::MAX; map.tiles.len()];
        let mut heap = BinaryHeap::new();
        if map.in_bounds(target.0, target.1) {
            dist[map.idx(target.0, target.1)] = 0;
            heap.push(Reverse((0u32, target.0, target.1)));
        }
        while let Some(Reverse((d, i, j))) = heap.pop() {
            if d > dist[map.idx(i, j)] || d > max_cost {
                continue;
            }
            for (di, dj, c) in NEIGHBOURS {
                if !step_ok(map, i, j, di, dj, true) {
                    continue;
                }
                let (ni, nj) = (i + di, j + dj);
                let k = map.idx(ni, nj);
                let nd = d + c;
                if nd < dist[k] {
                    dist[k] = nd;
                    heap.push(Reverse((nd, ni, nj)));
                }
            }
        }
        FlowField { w: map.w, h: map.h, dist, origin: target }
    }

    pub fn get(&self, i: i32, j: i32) -> u32 {
        if i < 0 || j < 0 || i >= self.w || j >= self.h { u32::MAX } else { self.dist[(j * self.w + i) as usize] }
    }

    /// Direction to move from `p` to get closer to the target.
    pub fn direction(&self, map: &TileMap, p: Vec2, flying: bool) -> Option<Vec2> {
        let (i, j) = map.tile_at(p);
        let here = self.get(i, j);
        let mut best = (here, None);
        for (di, dj, _) in NEIGHBOURS {
            if !step_ok(map, i, j, di, dj, flying) {
                continue;
            }
            let d = self.get(i + di, j + dj);
            if d < best.0 {
                best = (d, Some((i + di, j + dj)));
            }
        }
        best.1.map(|(ti, tj)| {
            let c = Vec2::new(ti as f32 + 0.5, tj as f32 + 0.5);
            (c - p).normalize_or_zero()
        })
    }
}

/// A* over tiles; returns waypoints (tile centres, last = exact goal) or None.
pub fn astar(map: &TileMap, from: Vec2, to: Vec2, max_nodes: usize) -> Option<Vec<Vec2>> {
    let s = map.tile_at(from);
    let g = map.tile_at(to);
    if !passable(map, g.0, g.1, false) {
        return None;
    }
    let n = map.tiles.len();
    let mut cost = vec![u32::MAX; n];
    let mut came = vec![usize::MAX; n];
    let h = |i: i32, j: i32| -> u32 {
        let dx = (i - g.0).unsigned_abs();
        let dy = (j - g.1).unsigned_abs();
        10 * dx.max(dy) + 4 * dx.min(dy)
    };
    let si = map.idx(s.0, s.1);
    cost[si] = 0;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((h(s.0, s.1), 0u32, s.0, s.1)));
    let mut expanded = 0;
    while let Some(Reverse((_, c, i, j))) = heap.pop() {
        if (i, j) == g {
            let mut out = vec![to];
            let mut k = map.idx(i, j);
            while came[k] != usize::MAX && came[k] != si {
                k = came[k];
                let (ti, tj) = ((k as i32) % map.w, (k as i32) / map.w);
                out.push(Vec2::new(ti as f32 + 0.5, tj as f32 + 0.5));
            }
            out.reverse();
            return Some(smooth(map, from, out));
        }
        if c > cost[map.idx(i, j)] {
            continue;
        }
        expanded += 1;
        if expanded > max_nodes {
            return None;
        }
        for (di, dj, sc) in NEIGHBOURS {
            if !step_ok(map, i, j, di, dj, false) {
                continue;
            }
            let (ni, nj) = (i + di, j + dj);
            let k = map.idx(ni, nj);
            let nc = c + sc;
            if nc < cost[k] {
                cost[k] = nc;
                came[k] = map.idx(i, j);
                heap.push(Reverse((nc + h(ni, nj), nc, ni, nj)));
            }
        }
    }
    None
}

/// Drop waypoints that can be skipped in a straight walkable line.
fn smooth(map: &TileMap, from: Vec2, pts: Vec<Vec2>) -> Vec<Vec2> {
    let mut out = Vec::new();
    let mut cur = from;
    let mut i = 0;
    while i < pts.len() {
        let mut j = pts.len() - 1;
        while j > i && !walk_line_clear(map, cur, pts[j]) {
            j -= 1;
        }
        out.push(pts[j]);
        cur = pts[j];
        i = j + 1;
    }
    out
}

/// Can a body walk straight from a to b (checks tiles along the line, with some clearance)?
pub fn walk_line_clear(map: &TileMap, a: Vec2, b: Vec2) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 1e-4 {
        return true;
    }
    let side = Vec2::new(-d.y, d.x) / len * 0.25;
    let steps = (len * 4.0).ceil() as i32;
    for k in 0..=steps {
        let p = a + d * (k as f32 / steps as f32);
        for q in [p, p + side, p - side] {
            if !map.walkable_at(q) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TileMap {
        let mut m = TileMap::new(10, 10);
        for j in 0..10 {
            for i in 0..10 {
                let edge = i == 0 || j == 0 || i == 9 || j == 9;
                m.set(i, j, if edge { Tile::Wall } else { Tile::Floor });
            }
        }
        for j in 1..8 {
            m.set(5, j, Tile::Wall);
        }
        m
    }

    #[test]
    fn astar_goes_around_walls() {
        let m = map();
        let p = astar(&m, Vec2::new(2.5, 2.5), Vec2::new(7.5, 2.5), 10_000).unwrap();
        assert!(p.iter().any(|w| w.y > 7.0), "path must dip below the wall: {p:?}");
        assert_eq!(*p.last().unwrap(), Vec2::new(7.5, 2.5));
    }

    #[test]
    fn flow_field_points_downhill() {
        let m = map();
        let f = FlowField::build(&m, (7, 2), 10_000);
        assert_eq!(f.get(7, 2), 0);
        let d = f.direction(&m, Vec2::new(2.5, 2.5), false).unwrap();
        assert!(d.y > 0.0, "should head south around the wall, got {d}");
    }
}
