//! Field of view: Albert Ford's symmetric shadowcasting on the tile grid.
//! <https://www.albertford.com/shadowcasting/>

use super::tile::TileMap;

/// Mark every tile visible from (ox, oy) within `radius` by calling `reveal`.
pub fn shadowcast(map: &TileMap, ox: i32, oy: i32, radius: i32, mut reveal: impl FnMut(i32, i32)) {
    reveal(ox, oy);
    let r2 = (radius * radius + radius) as i64;
    for quadrant in 0..4 {
        let transform = |row: i32, col: i32| -> (i32, i32) {
            match quadrant {
                0 => (ox + col, oy - row), // north
                1 => (ox + row, oy + col), // east
                2 => (ox + col, oy + row), // south
                _ => (ox - row, oy + col), // west
            }
        };
        let blocks = |row: i32, col: i32| {
            let (x, y) = transform(row, col);
            map.get(x, y).blocks_sight()
        };
        // Explicit stack instead of recursion: (depth, start_slope, end_slope).
        let mut stack: Vec<(i32, f64, f64)> = vec![(1, -1.0, 1.0)];
        while let Some((depth, mut start, end)) = stack.pop() {
            if depth > radius {
                continue;
            }
            let min_col = round_ties_up(depth as f64 * start);
            let max_col = round_ties_down(depth as f64 * end);
            let mut prev: Option<bool> = None; // Some(true) = previous tile was a wall
            for col in min_col..=max_col {
                let wall = blocks(depth, col);
                let symmetric = col as f64 >= depth as f64 * start && col as f64 <= depth as f64 * end;
                if (wall || symmetric) && (depth as i64 * depth as i64 + col as i64 * col as i64) <= r2 {
                    let (x, y) = transform(depth, col);
                    reveal(x, y);
                }
                if prev == Some(true) && !wall {
                    start = slope(depth, col);
                }
                if prev == Some(false) && wall {
                    stack.push((depth + 1, start, slope(depth, col)));
                }
                prev = Some(wall);
            }
            if prev == Some(false) {
                stack.push((depth + 1, start, end));
            }
        }
    }
}

fn slope(depth: i32, col: i32) -> f64 {
    (2 * col - 1) as f64 / (2 * depth) as f64
}

fn round_ties_up(n: f64) -> i32 {
    (n + 0.5).floor() as i32
}

fn round_ties_down(n: f64) -> i32 {
    (n - 0.5).ceil() as i32
}

/// Visibility mask (one bool per tile) from a point.
pub fn visible_from(map: &TileMap, ox: i32, oy: i32, radius: i32) -> Vec<bool> {
    let mut vis = vec![false; map.tiles.len()];
    shadowcast(map, ox, oy, radius, |x, y| {
        if map.in_bounds(x, y) {
            vis[map.idx(x, y)] = true;
        }
    });
    vis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tile::Tile;

    fn room() -> TileMap {
        let mut m = TileMap::new(12, 12);
        for j in 0..12 {
            for i in 0..12 {
                let edge = i == 0 || j == 0 || i == 11 || j == 11;
                m.set(i, j, if edge { Tile::Wall } else { Tile::Floor });
            }
        }
        m
    }

    #[test]
    fn walls_block_sight() {
        let mut m = room();
        for j in 1..11 {
            m.set(6, j, Tile::Wall);
        }
        let v = visible_from(&m, 3, 5, 20);
        assert!(v[m.idx(5, 5)]);
        assert!(v[m.idx(6, 5)], "the wall itself is visible");
        assert!(!v[m.idx(8, 5)], "behind the wall is hidden");
    }

    #[test]
    fn symmetric() {
        let mut m = room();
        m.set(5, 4, Tile::Pillar);
        m.set(7, 7, Tile::Pillar);
        m.set(3, 8, Tile::Wall);
        for (ax, ay) in [(2, 2), (4, 6), (9, 9), (6, 3)] {
            let va = visible_from(&m, ax, ay, 20);
            for by in 1..11 {
                for bx in 1..11 {
                    if m.get(bx, by).blocks_sight() || m.get(ax, ay).blocks_sight() {
                        continue;
                    }
                    let vb = visible_from(&m, bx, by, 20);
                    assert_eq!(va[m.idx(bx, by)], vb[m.idx(ax, ay)], "({ax},{ay}) <-> ({bx},{by})");
                }
            }
        }
    }

    #[test]
    fn closed_doors_block() {
        let mut m = room();
        for j in 1..11 {
            m.set(6, j, Tile::Wall);
        }
        m.set(6, 5, Tile::DoorClosed);
        assert!(!visible_from(&m, 3, 5, 20)[m.idx(8, 5)]);
        m.set(6, 5, Tile::DoorOpen);
        assert!(visible_from(&m, 3, 5, 20)[m.idx(8, 5)]);
    }
}
