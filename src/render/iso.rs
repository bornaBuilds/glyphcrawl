//! Isometric projection — the single source of truth used by stamps, the 3D raycaster,
//! mouse picking and ground decals.
//!
//! World axes: `x` and `z` on the ground (1 tile = 1 unit), `y` up. The camera looks down at
//! 30° from the +x+z side (2:1 dimetric). With terminal cells twice as tall as wide:
//!
//! ```text
//! col = 8 (x - z)
//! row = 2 (x + z) - 2√6 · y        (2√6 ≈ 4.899 rows per unit of height)
//! ```
//!
//! So a floor tile is a 16×4-cell diamond and a 1.8-unit hero is ~9 rows tall.
//! At a fixed screen position, a surface that is *higher* is *nearer* the camera, so the
//! depth buffer is simply a height buffer.

use glam::{Vec2, Vec3};

pub const COLS_PER_UNIT: f32 = 8.0;
pub const ROWS_PER_UNIT: f32 = 2.0;
/// Rows per unit of height: 2·√6.
pub const ROWS_PER_Y: f32 = 4.898_979_5;

/// Direction the camera looks (into the screen), unit length.
pub const VIEW_DIR: Vec3 = Vec3::new(-0.612_372_4, -0.5, -0.612_372_4);
/// Screen-right in world space (unit length).
pub const SCREEN_RIGHT: Vec3 = Vec3::new(0.707_106_77, 0.0, -0.707_106_77);
/// Screen-up in world space (unit length).
pub const SCREEN_UP: Vec3 = Vec3::new(-0.353_553_4, 0.866_025_4, -0.353_553_4);

/// Project a world point to absolute "world-screen" cell coordinates (fractional).
#[inline]
pub fn world_to_screen(p: Vec3) -> Vec2 {
    Vec2::new(COLS_PER_UNIT * (p.x - p.z), ROWS_PER_UNIT * (p.x + p.z) - ROWS_PER_Y * p.y)
}

/// Inverse projection onto the horizontal plane at height `h`. Returns (x, z).
#[inline]
pub fn screen_to_plane(s: Vec2, h: f32) -> Vec2 {
    let sum = (s.y + ROWS_PER_Y * h) / ROWS_PER_UNIT; // x + z
    let diff = s.x / COLS_PER_UNIT; // x - z
    Vec2::new((sum + diff) * 0.5, (sum - diff) * 0.5)
}

/// Screen position of a tile's top vertex (tile (i, j) covers [i, i+1] × [j, j+1]).
#[inline]
pub fn tile_top(i: i32, j: i32) -> (i32, i32) {
    (8 * (i - j), 2 * (i + j))
}

/// Ground-plane direction for a screen-space direction (e.g. WASD). `up` = towards top of screen.
pub fn screen_dir_to_world(right: f32, down: f32) -> Vec2 {
    // Screen right = +x −z, screen down = +x +z.
    let v = Vec2::new(right + down, -right + down);
    if v.length_squared() > 0.0 { v.normalize() } else { v }
}

/// The camera: integer offset from world-screen coordinates to grid cells.
#[derive(Clone, Copy, Debug, Default)]
pub struct Camera {
    pub col: i32,
    pub row: i32,
}

impl Camera {
    /// World-screen coords -> grid cell coords.
    #[inline]
    pub fn to_grid(&self, s: Vec2) -> Vec2 {
        Vec2::new(s.x - self.col as f32, s.y - self.row as f32)
    }

    #[inline]
    pub fn world_to_grid(&self, p: Vec3) -> Vec2 {
        self.to_grid(world_to_screen(p))
    }

    /// Grid cell coords (fractional) -> ground point at height h.
    #[inline]
    pub fn grid_to_plane(&self, g: Vec2, h: f32) -> Vec2 {
        screen_to_plane(Vec2::new(g.x + self.col as f32, g.y + self.row as f32), h)
    }

    /// Centre the camera on a world point, snapped to whole cells.
    pub fn center_on(&mut self, p: Vec3, grid_w: u16, grid_h: u16) {
        let s = world_to_screen(p);
        self.col = (s.x - grid_w as f32 / 2.0).round() as i32;
        self.row = (s.y - grid_h as f32 / 2.0).round() as i32;
    }

    /// Follow a point with a dead zone (in cells) so small moves don't pan the screen.
    pub fn follow(&mut self, p: Vec3, grid_w: u16, grid_h: u16, dead_x: i32, dead_y: i32) {
        let g = self.world_to_grid(p);
        let (cx, cy) = (grid_w as f32 / 2.0, grid_h as f32 / 2.0);
        let dx = g.x - cx;
        let dy = g.y - cy;
        if dx > dead_x as f32 {
            self.col += (dx - dead_x as f32).round() as i32;
        } else if dx < -dead_x as f32 {
            self.col += (dx + dead_x as f32).round() as i32;
        }
        if dy > dead_y as f32 {
            self.row += (dy - dead_y as f32).round() as i32;
        } else if dy < -dead_y as f32 {
            self.row += (dy + dead_y as f32).round() as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_on_ground() {
        for &(x, z) in &[(0.0, 0.0), (3.25, -1.5), (10.0, 7.75), (-4.0, 2.0)] {
            let s = world_to_screen(Vec3::new(x, 0.0, z));
            let p = screen_to_plane(s, 0.0);
            assert!((p.x - x).abs() < 1e-4 && (p.y - z).abs() < 1e-4);
        }
    }

    #[test]
    fn round_trip_at_height() {
        let p = Vec3::new(2.0, 1.3, 5.0);
        let s = world_to_screen(p);
        let q = screen_to_plane(s, 1.3);
        assert!((q.x - 2.0).abs() < 1e-4 && (q.y - 5.0).abs() < 1e-4);
    }

    #[test]
    fn view_dir_keeps_screen_position() {
        let p = Vec3::new(1.0, 0.5, 2.0);
        let a = world_to_screen(p);
        let b = world_to_screen(p + VIEW_DIR * 3.0);
        assert!((a - b).length() < 1e-4);
        // Moving into the screen lowers y: higher = nearer.
        assert!((p + VIEW_DIR).y < p.y);
    }

    #[test]
    fn screen_basis_matches_projection() {
        let o = world_to_screen(Vec3::ZERO);
        let r = world_to_screen(SCREEN_RIGHT);
        let u = world_to_screen(SCREEN_UP);
        assert!((r.y - o.y).abs() < 1e-4 && r.x > o.x);
        assert!((u.x - o.x).abs() < 1e-4 && u.y < o.y);
    }

    #[test]
    fn tile_diamond_is_16_by_4() {
        let (c0, r0) = tile_top(0, 0);
        let right = world_to_screen(Vec3::new(1.0, 0.0, 0.0));
        let bottom = world_to_screen(Vec3::new(1.0, 0.0, 1.0));
        let left = world_to_screen(Vec3::new(0.0, 0.0, 1.0));
        assert_eq!((c0, r0), (0, 0));
        assert_eq!((right.x - left.x) as i32, 16);
        assert_eq!(bottom.y as i32, 4);
    }
}
