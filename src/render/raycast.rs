//! CPU raycaster for figure parts. Every screen cell gets 2×4 samples (square sub-pixels);
//! each sample stores the nearest surface hit (height, normal, material, owner).
//! The projection is orthographic, so rays are parallel and intersections are exact —
//! no triangles, no meshes.

use glam::{Mat3, Vec2, Vec3};

use super::figure::{Figure, Pose, Shape};
use super::iso::{Camera, VIEW_DIR};

pub const SX: usize = 2;
pub const SY: usize = 4;

/// How far above the ground rays start (along -VIEW_DIR).
const RAY_START: f32 = 30.0;

#[derive(Default)]
pub struct SampleBuffer {
    pub w: usize,
    pub h: usize,
    pub y: Vec<f32>,
    pub n: Vec<Vec3>,
    pub p: Vec<Vec3>,
    pub mat: Vec<u16>,
    /// 0 = empty, otherwise index+1 into the frame's figure list.
    pub owner: Vec<u16>,
    pub occ: Vec<bool>,
    /// Bounding box of written samples: (x0, y0, x1, y1), exclusive max.
    pub dirty: Option<(usize, usize, usize, usize)>,
}


impl SampleBuffer {
    /// Prepare for a grid of `gw`×`gh` cells; clears whatever was written last frame.
    pub fn begin(&mut self, gw: u16, gh: u16) {
        let (w, h) = (gw as usize * SX, gh as usize * SY);
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            let n = w * h;
            self.y = vec![f32::NEG_INFINITY; n];
            self.n = vec![Vec3::ZERO; n];
            self.p = vec![Vec3::ZERO; n];
            self.mat = vec![0; n];
            self.owner = vec![0; n];
            self.occ = vec![false; n];
            self.dirty = None;
            return;
        }
        if let Some((x0, y0, x1, y1)) = self.dirty.take() {
            for yy in y0..y1 {
                let r = yy * self.w;
                self.y[r + x0..r + x1].fill(f32::NEG_INFINITY);
                self.owner[r + x0..r + x1].fill(0);
                self.occ[r + x0..r + x1].fill(false);
            }
        }
    }

    fn mark(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        self.dirty = Some(match self.dirty {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        });
    }
}

/// Ray vs unit primitive in local space. Returns (t, local normal) of the entry point.
fn intersect(shape: Shape, o: Vec3, d: Vec3) -> Option<(f32, Vec3)> {
    match shape {
        Shape::Box => {
            let mut tmin = f32::NEG_INFINITY;
            let mut tmax = f32::INFINITY;
            let mut axis = 0;
            for a in 0..3 {
                let (oa, da) = (o[a], d[a]);
                if da.abs() < 1e-9 {
                    if oa.abs() > 0.5 {
                        return None;
                    }
                    continue;
                }
                let t1 = (-0.5 - oa) / da;
                let t2 = (0.5 - oa) / da;
                let (lo, hi) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
                if lo > tmin {
                    tmin = lo;
                    axis = a;
                }
                tmax = tmax.min(hi);
            }
            if tmin > tmax || tmax < 0.0 {
                return None;
            }
            let mut n = Vec3::ZERO;
            n[axis] = -d[axis].signum();
            Some((tmin, n))
        }
        Shape::Sphere => {
            let a = d.dot(d);
            let b = o.dot(d);
            let c = o.dot(o) - 0.25;
            let disc = b * b - a * c;
            if disc < 0.0 {
                return None;
            }
            let t = (-b - disc.sqrt()) / a;
            Some((t, (o + d * t) * 2.0))
        }
        Shape::Cylinder => {
            let a = d.x * d.x + d.z * d.z;
            let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
            if a > 1e-12 {
                let b = o.x * d.x + o.z * d.z;
                let c = o.x * o.x + o.z * o.z - 0.25;
                let disc = b * b - a * c;
                if disc < 0.0 {
                    return None;
                }
                let s = disc.sqrt();
                t0 = (-b - s) / a;
                t1 = (-b + s) / a;
            } else if o.x * o.x + o.z * o.z > 0.25 {
                return None;
            }
            let (ty0, ty1) = if d.y.abs() < 1e-9 {
                if o.y.abs() > 0.5 {
                    return None;
                }
                (f32::NEG_INFINITY, f32::INFINITY)
            } else {
                let u = (-0.5 - o.y) / d.y;
                let v = (0.5 - o.y) / d.y;
                if u < v { (u, v) } else { (v, u) }
            };
            let tin = t0.max(ty0);
            let tout = t1.min(ty1);
            if tin > tout {
                return None;
            }
            let p = o + d * tin;
            let n = if t0 >= ty0 { Vec3::new(p.x, 0.0, p.z) * 2.0 } else { Vec3::new(0.0, -d.y.signum(), 0.0) };
            Some((tin, n))
        }
        Shape::Cone => {
            // Apex at y = 0.5, base radius 0.5 at y = -0.5.
            let k2 = 0.25;
            let yo = 0.5 - o.y;
            let a = d.x * d.x + d.z * d.z - k2 * d.y * d.y;
            let b = o.x * d.x + o.z * d.z + k2 * yo * d.y;
            let c = o.x * o.x + o.z * o.z - k2 * yo * yo;
            let mut best: Option<(f32, Vec3)> = None;
            let mut consider = |t: f32, n: Vec3| {
                if best.is_none_or(|(bt, _)| t < bt) {
                    best = Some((t, n));
                }
            };
            let in_height = |t: f32| {
                let y = o.y + d.y * t;
                (-0.5..=0.5).contains(&y)
            };
            if a.abs() > 1e-9 {
                let disc = b * b - a * c;
                if disc >= 0.0 {
                    let s = disc.sqrt();
                    for t in [(-b - s) / a, (-b + s) / a] {
                        if in_height(t) {
                            let p = o + d * t;
                            consider(t, Vec3::new(p.x, k2 * (0.5 - p.y), p.z));
                        }
                    }
                }
            } else if b.abs() > 1e-9 {
                let t = -c / (2.0 * b);
                if in_height(t) {
                    let p = o + d * t;
                    consider(t, Vec3::new(p.x, k2 * (0.5 - p.y), p.z));
                }
            }
            if d.y.abs() > 1e-9 {
                let t = (-0.5 - o.y) / d.y;
                let p = o + d * t;
                if p.x * p.x + p.z * p.z <= 0.25 {
                    consider(t, Vec3::NEG_Y);
                }
            }
            best
        }
        Shape::Octahedron => {
            let mut tmin = f32::NEG_INFINITY;
            let mut tmax = f32::INFINITY;
            let mut nmin = Vec3::Y;
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    for sz in [-1.0f32, 1.0] {
                        let n = Vec3::new(sx, sy, sz);
                        let denom = n.dot(d);
                        let dist = 0.5 - n.dot(o);
                        if denom.abs() < 1e-9 {
                            if dist < 0.0 {
                                return None;
                            }
                            continue;
                        }
                        let t = dist / denom;
                        if denom < 0.0 {
                            if t > tmin {
                                tmin = t;
                                nmin = n;
                            }
                        } else {
                            tmax = tmax.min(t);
                        }
                    }
                }
            }
            if tmin > tmax {
                return None;
            }
            Some((tmin, nmin))
        }
    }
}

/// Raycast every part of a posed figure into the sample buffer.
pub fn raycast_figure(buf: &mut SampleBuffer, cam: &Camera, fig: &Figure, pose: &Pose, owner: u16) {
    if buf.w == 0 {
        return;
    }
    let d_w = VIEW_DIR;
    for (i, part) in fig.parts.iter().enumerate() {
        if part.hidden {
            continue;
        }
        let m = pose.shape[i];
        let det = m.determinant();
        if det.abs() < 1e-9 {
            continue;
        }
        let inv = m.inverse();
        let normal_mat = Mat3::from_mat4(inv).transpose();
        // Screen bounding box of the part's unit cube.
        let mut lo = Vec2::splat(f32::INFINITY);
        let mut hi = Vec2::splat(f32::NEG_INFINITY);
        for c in 0..8 {
            let corner = Vec3::new(
                if c & 1 == 0 { -0.5 } else { 0.5 },
                if c & 2 == 0 { -0.5 } else { 0.5 },
                if c & 4 == 0 { -0.5 } else { 0.5 },
            );
            let g = cam.world_to_grid(m.transform_point3(corner));
            lo = lo.min(g);
            hi = hi.max(g);
        }
        let sx0 = ((lo.x * SX as f32).floor().max(0.0)) as usize;
        let sy0 = ((lo.y * SY as f32).floor().max(0.0)) as usize;
        let sx1 = ((hi.x * SX as f32).ceil().max(0.0) as usize).min(buf.w);
        let sy1 = ((hi.y * SY as f32).ceil().max(0.0) as usize).min(buf.h);
        if sx0 >= sx1 || sy0 >= sy1 {
            continue;
        }
        // The ray origin is affine in the sample coordinates: precompute origin + steps.
        let origin_at = |sx: f32, sy: f32| -> Vec3 {
            let g = Vec2::new(sx / SX as f32, sy / SY as f32);
            let ground = cam.grid_to_plane(g, 0.0);
            Vec3::new(ground.x, 0.0, ground.y) - d_w * RAY_START
        };
        let o00 = origin_at(sx0 as f32 + 0.5, sy0 as f32 + 0.5);
        let ox = origin_at(sx0 as f32 + 1.5, sy0 as f32 + 0.5) - o00;
        let oy = origin_at(sx0 as f32 + 0.5, sy0 as f32 + 1.5) - o00;
        let l00 = inv.transform_point3(o00);
        let lx = inv.transform_vector3(ox);
        let ly = inv.transform_vector3(oy);
        let ld = inv.transform_vector3(d_w);
        let mat = fig.mat[i];
        let mut wrote = false;
        for sy in sy0..sy1 {
            let row = sy * buf.w;
            let base = l00 + ly * (sy - sy0) as f32;
            let wbase = o00 + oy * (sy - sy0) as f32;
            for sx in sx0..sx1 {
                let k = (sx - sx0) as f32;
                let lo_ = base + lx * k;
                let Some((t, nl)) = intersect(part.shape, lo_, ld) else { continue };
                let wp = wbase + ox * k + d_w * t;
                let idx = row + sx;
                if wp.y > buf.y[idx] {
                    buf.y[idx] = wp.y;
                    buf.p[idx] = wp;
                    buf.n[idx] = (normal_mat * nl).normalize_or_zero();
                    buf.mat[idx] = mat;
                    buf.owner[idx] = owner;
                    wrote = true;
                }
            }
        }
        if wrote {
            buf.mark(sx0, sy0, sx1, sy1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_hit_from_above() {
        let d = Vec3::new(0.0, -1.0, 0.0);
        let o = Vec3::new(0.03, 5.0, 0.02);
        for s in [Shape::Box, Shape::Sphere, Shape::Cylinder, Shape::Cone, Shape::Octahedron] {
            let (t, n) = intersect(s, o, d).unwrap_or_else(|| panic!("{s:?} missed"));
            assert!((4.4..=4.6).contains(&t), "{s:?} t={t}");
            assert!(n.normalize().y > 0.4, "{s:?} normal {n}");
        }
    }

    #[test]
    fn misses_outside() {
        let d = Vec3::new(0.0, -1.0, 0.0);
        let o = Vec3::new(0.9, 5.0, 0.0);
        for s in [Shape::Box, Shape::Sphere, Shape::Cylinder, Shape::Cone, Shape::Octahedron] {
            assert!(intersect(s, o, d).is_none(), "{s:?} should miss");
        }
    }

    #[test]
    fn cone_side_hit() {
        // Horizontal ray at y = -0.25 hits the cone where its radius is 0.375.
        let (t, n) = intersect(Shape::Cone, Vec3::new(-5.0, -0.25, 0.0), Vec3::X).unwrap();
        assert!((t - (5.0 - 0.375)).abs() < 1e-3, "t={t}");
        assert!(n.x < 0.0 && n.y > 0.0);
    }
}
