//! The tile grid. Tile (i, j) covers world [i, i+1] × [j, j+1] on the ground plane.

use glam::Vec2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tile {
    Void,
    Floor,
    Wall,
    /// Wall with a torch mounted on it (emits light).
    WallTorch,
    DoorOpen,
    DoorClosed,
    Pit,
    Pillar,
    /// Free-standing fire bowl (emits light, blocks movement but not sight).
    Brazier,
    /// Reward pedestal.
    Chest,
}

impl Tile {
    pub fn walkable(self) -> bool {
        matches!(self, Tile::Floor | Tile::DoorOpen)
    }

    pub fn blocks_sight(self) -> bool {
        matches!(self, Tile::Void | Tile::Wall | Tile::WallTorch | Tile::DoorClosed | Tile::Pillar)
    }

    /// Projectiles fly over pits and floors; walls, doors, pillars and props stop them.
    pub fn blocks_projectiles(self) -> bool {
        matches!(self, Tile::Void | Tile::Wall | Tile::WallTorch | Tile::DoorClosed | Tile::Pillar)
    }

    /// Wall-like tiles that get drawn as short "stumps" when they'd hide floor behind them.
    pub fn is_wallish(self) -> bool {
        matches!(self, Tile::Wall | Tile::WallTorch | Tile::DoorClosed)
    }

    /// Interior space (anything you can see across inside a room).
    pub fn is_open(self) -> bool {
        !matches!(self, Tile::Void | Tile::Wall | Tile::WallTorch | Tile::DoorClosed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileMap {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
}

impl TileMap {
    pub fn new(w: i32, h: i32) -> TileMap {
        TileMap { w, h, tiles: vec![Tile::Void; (w * h) as usize] }
    }

    #[inline]
    pub fn in_bounds(&self, i: i32, j: i32) -> bool {
        i >= 0 && j >= 0 && i < self.w && j < self.h
    }

    #[inline]
    pub fn idx(&self, i: i32, j: i32) -> usize {
        (j * self.w + i) as usize
    }

    #[inline]
    pub fn get(&self, i: i32, j: i32) -> Tile {
        if self.in_bounds(i, j) { self.tiles[self.idx(i, j)] } else { Tile::Void }
    }

    pub fn set(&mut self, i: i32, j: i32, t: Tile) {
        if self.in_bounds(i, j) {
            let k = self.idx(i, j);
            self.tiles[k] = t;
        }
    }

    pub fn tile_at(&self, p: Vec2) -> (i32, i32) {
        (p.x.floor() as i32, p.y.floor() as i32)
    }

    pub fn walkable_at(&self, p: Vec2) -> bool {
        let (i, j) = self.tile_at(p);
        self.get(i, j).walkable()
    }

    /// Should the wall at (i, j) be drawn short because open floor sits behind it?
    pub fn is_front_wall(&self, i: i32, j: i32) -> bool {
        self.get(i - 1, j).is_open() || self.get(i, j - 1).is_open() || self.get(i - 1, j - 1).is_open()
    }

    /// Line of sight between two ground points (samples the tiles along the segment).
    pub fn line_clear(&self, a: Vec2, b: Vec2, block: impl Fn(Tile) -> bool) -> bool {
        let d = b - a;
        let steps = (d.length() * 4.0).ceil().max(1.0) as i32;
        for k in 1..steps {
            let p = a + d * (k as f32 / steps as f32);
            let (i, j) = self.tile_at(p);
            if block(self.get(i, j)) {
                return false;
            }
        }
        true
    }

    /// Deterministic per-tile hash for picking art variants.
    pub fn hash(i: i32, j: i32) -> u32 {
        // murmur3 finaliser over both coordinates: no visible stripes.
        let mut x = (i as u32 as u64) | ((j as u32 as u64) << 32);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^= x >> 33;
        x as u32
    }
}

/// Push a circle (centre `p`, radius `r`) out of every non-walkable tile it overlaps.
pub fn collide_circle(map: &TileMap, mut p: Vec2, r: f32, solid: impl Fn(Tile) -> bool) -> Vec2 {
    for _ in 0..3 {
        let (ci, cj) = map.tile_at(p);
        let mut moved = false;
        for j in cj - 1..=cj + 1 {
            for i in ci - 1..=ci + 1 {
                if !solid(map.get(i, j)) {
                    continue;
                }
                let closest = Vec2::new(p.x.clamp(i as f32, i as f32 + 1.0), p.y.clamp(j as f32, j as f32 + 1.0));
                let d = p - closest;
                let dist2 = d.length_squared();
                if dist2 < r * r {
                    if dist2 > 1e-8 {
                        let dist = dist2.sqrt();
                        p += d / dist * (r - dist);
                    } else {
                        // Centre inside the tile: push out along the shortest axis.
                        let cx = i as f32 + 0.5;
                        let cy = j as f32 + 0.5;
                        let dx = p.x - cx;
                        let dy = p.y - cy;
                        if dx.abs() > dy.abs() {
                            p.x = if dx > 0.0 { i as f32 + 1.0 + r } else { i as f32 - r };
                        } else {
                            p.y = if dy > 0.0 { j as f32 + 1.0 + r } else { j as f32 - r };
                        }
                    }
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_is_pushed_out_of_walls() {
        let mut m = TileMap::new(3, 3);
        for t in &mut m.tiles {
            *t = Tile::Floor;
        }
        m.set(1, 0, Tile::Wall);
        let p = collide_circle(&m, Vec2::new(1.5, 1.1), 0.3, |t| !t.walkable());
        assert!(p.y >= 1.3 - 1e-4);
    }

    #[test]
    fn front_wall_rule() {
        let mut m = TileMap::new(4, 4);
        for j in 0..4 {
            for i in 0..4 {
                let edge = i == 0 || j == 0 || i == 3 || j == 3;
                m.set(i, j, if edge { Tile::Wall } else { Tile::Floor });
            }
        }
        assert!(!m.is_front_wall(0, 2)); // back (left) wall
        assert!(!m.is_front_wall(2, 0)); // back (top) wall
        assert!(m.is_front_wall(3, 2)); // right wall hides floor
        assert!(m.is_front_wall(2, 3)); // bottom wall hides floor
    }
}
