//! Render-only effects driven by `GameEvent`s: glyph particles, floating damage numbers,
//! slash trails, screen shake, and ground decals (telegraphs, burning ground, markers).

use glam::{Vec2, Vec3};

use super::cell::{Cell, CellGrid, Rgb};
use super::iso::Camera;
use super::world::WorldLayer;
use crate::game::defs::AreaFx;
use crate::game::world::{FxKind, GameEvent};

#[derive(Clone, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub life: f32,
    pub max: f32,
    pub glyphs: &'static str,
    pub color: Rgb,
    pub gravity: f32,
    pub drag: f32,
}

#[derive(Clone, Debug)]
struct FloatText {
    pos: Vec3,
    text: String,
    color: Rgb,
    life: f32,
}

#[derive(Clone, Debug)]
struct SlashTrail {
    pos: Vec2,
    dir: Vec2,
    range: f32,
    arc: f32,
    t: f32,
    hero: bool,
}

#[derive(Default)]
pub struct Fx {
    pub particles: Vec<Particle>,
    texts: Vec<FloatText>,
    slashes: Vec<SlashTrail>,
    pub shake: f32,
    seed: u32,
}

const FIRE: Rgb = Rgb(255, 150, 50);
const FIRE2: Rgb = Rgb(255, 80, 30);
const FROST: Rgb = Rgb(160, 225, 255);
const BONE: Rgb = Rgb(217, 208, 180);
const DUST: Rgb = Rgb(150, 130, 105);
const GREEN: Rgb = Rgb(120, 220, 90);
const ARCANE: Rgb = Rgb(200, 150, 255);
const SMOKE: Rgb = Rgb(120, 120, 130);

impl Fx {
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        if self.seed == 0 {
            self.seed = 0x1234_5678;
        }
        (self.seed % 10_000) as f32 / 10_000.0
    }

    fn rs(&mut self) -> f32 {
        self.rand() * 2.0 - 1.0
    }

    #[allow(clippy::too_many_arguments)]
    fn burst(&mut self, at: Vec3, n: usize, speed: f32, up: f32, life: f32, glyphs: &'static str, color: Rgb, gravity: f32) {
        for _ in 0..n {
            let a = self.rand() * std::f32::consts::TAU;
            let s = speed * (0.4 + 0.6 * self.rand());
            let vel = Vec3::new(a.cos() * s, up * (0.5 + self.rand()), a.sin() * s);
            let l = life * (0.6 + 0.4 * self.rand());
            self.particles.push(Particle { pos: at, vel, life: l, max: l, glyphs, color, gravity, drag: 2.0 });
        }
    }

    /// A ring of particles on the ground expanding outwards.
    fn ring(&mut self, at: Vec2, radius: f32, n: usize, glyphs: &'static str, color: Rgb, life: f32) {
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU + self.rand() * 0.2;
            let dir = Vec3::new(a.cos(), 0.0, a.sin());
            let pos = Vec3::new(at.x, 0.15, at.y) + dir * radius * 0.3;
            let vel = dir * radius * 1.8 + Vec3::Y * 0.6 * self.rand();
            self.particles.push(Particle { pos, vel, life, max: life, glyphs, color, gravity: 1.0, drag: 3.0 });
        }
    }

    pub fn on_event(&mut self, e: &GameEvent) {
        match e {
            GameEvent::Damage { pos, amount, crit, to_hero, blocked } => {
                let (text, color) = if *blocked {
                    (if *amount < 0.5 { "block".to_string() } else { format!("{:.0}", amount) }, Rgb(150, 170, 200))
                } else if *to_hero {
                    (format!("-{:.0}", amount), Rgb(255, 80, 80))
                } else if *crit {
                    (format!("{:.0}!", amount), Rgb(255, 225, 80))
                } else {
                    (format!("{:.0}", amount), Rgb(235, 235, 235))
                };
                if *amount >= 0.5 || *blocked {
                    let jitter = Vec3::new(self.rs() * 0.3, 0.0, self.rs() * 0.3);
                    self.texts.push(FloatText { pos: *pos + jitter, text, color, life: 0.9 });
                }
            }
            GameEvent::Slash { pos, dir, range, arc, hero } => {
                self.slashes.push(SlashTrail { pos: *pos, dir: *dir, range: *range, arc: *arc, t: 0.0, hero: *hero });
            }
            GameEvent::Fx { kind, pos, radius } => {
                let p3 = Vec3::new(pos.x, 0.6, pos.y);
                match kind {
                    FxKind::Hit => self.burst(p3 + Vec3::Y * 0.5, 4, 3.0, 2.0, 0.25, "*'.", Rgb(255, 230, 170), 6.0),
                    FxKind::Area(fx) => match fx {
                        AreaFx::Fire | AreaFx::Blast => {
                            self.ring(*pos, *radius, 28, "*o.", FIRE, 0.45);
                            self.burst(p3, 16, 2.0, 3.0, 0.6, "^*.", FIRE2, 2.0);
                        }
                        AreaFx::Frost => {
                            self.ring(*pos, *radius, 32, "*+.", FROST, 0.55);
                            self.burst(p3, 10, 1.0, 1.5, 0.8, "*.", Rgb(220, 245, 255), 0.5);
                        }
                        AreaFx::Arrows => {
                            for _ in 0..7 {
                                let off = Vec2::new(self.rs(), self.rs()) * *radius * 0.8;
                                let p = Vec3::new(pos.x + off.x, 2.6, pos.y + off.y);
                                self.particles.push(Particle {
                                    pos: p,
                                    vel: Vec3::new(0.0, -9.0, 0.0),
                                    life: 0.28,
                                    max: 0.28,
                                    glyphs: "|",
                                    color: Rgb(200, 160, 110),
                                    gravity: 0.0,
                                    drag: 0.0,
                                });
                            }
                        }
                        AreaFx::Slam => {
                            self.ring(*pos, *radius, 36, "#=-.", DUST, 0.5);
                            self.burst(p3, 12, 2.5, 3.5, 0.7, "%.'", BONE, 8.0);
                        }
                        AreaFx::Splash => self.ring(*pos, *radius, 14, "o.", GREEN, 0.35),
                        AreaFx::Poison => self.ring(*pos, *radius, 16, "o.", GREEN, 0.5),
                        AreaFx::Smoke => {
                            for _ in 0..26 {
                                let off = Vec3::new(self.rs(), 0.2 + self.rand() * 0.8, self.rs()) * *radius * 0.7;
                                let l = 1.2 + self.rand() * 0.8;
                                let vel = Vec3::new(self.rs() * 0.3, 0.4, self.rs() * 0.3);
                                self.particles.push(Particle {
                                    pos: Vec3::new(pos.x, 0.3, pos.y) + off,
                                    vel,
                                    life: l,
                                    max: l,
                                    glyphs: "@%o:.",
                                    color: SMOKE,
                                    gravity: 0.0,
                                    drag: 1.0,
                                });
                            }
                        }
                    },
                    FxKind::Blink => self.burst(p3 + Vec3::Y * 0.4, 14, 2.0, 1.5, 0.45, "*:.", ARCANE, 0.0),
                    FxKind::Spawn => self.ring(*pos, 0.8, 14, ":.", DUST, 0.6),
                    FxKind::Pickup => self.burst(p3, 8, 1.0, 2.5, 0.6, "+*.", Rgb(110, 190, 255), -1.0),
                    FxKind::Heal => self.burst(p3, 8, 1.0, 2.5, 0.6, "+.", Rgb(255, 110, 130), -1.0),
                    FxKind::Dust => self.burst(Vec3::new(pos.x, 0.1, pos.y), 5, 1.2, 0.8, 0.35, ".'", DUST, 2.0),
                    FxKind::Bones => self.burst(p3, 10, 2.5, 3.0, 0.8, "%-o'.", BONE, 9.0),
                    FxKind::Splat => self.burst(p3, 12, 2.5, 2.0, 0.6, "o.", GREEN, 8.0),
                }
            }
            GameEvent::Shake(v) => self.shake = self.shake.max(*v),
            _ => {}
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.life -= dt;
            p.vel.y -= p.gravity * dt;
            p.vel *= (-p.drag * dt).exp();
            p.pos += p.vel * dt;
            if p.pos.y < 0.0 && p.gravity > 0.0 {
                p.pos.y = 0.0;
                p.vel.y = -p.vel.y * 0.3;
            }
        }
        self.particles.retain(|p| p.life > 0.0);
        for t in &mut self.texts {
            t.life -= dt;
            t.pos.y += dt * 1.1;
        }
        self.texts.retain(|t| t.life > 0.0);
        for s in &mut self.slashes {
            s.t += dt;
        }
        self.slashes.retain(|s| s.t < 0.16);
        self.shake = (self.shake - dt).max(0.0);
    }

    /// Camera offset in cells for screen shake.
    pub fn shake_offset(&mut self) -> (i32, i32) {
        if self.shake <= 0.0 {
            return (0, 0);
        }
        let k = (self.shake * 10.0).min(3.0);
        ((self.rs() * k).round() as i32, (self.rs() * k * 0.5).round() as i32)
    }

    pub fn draw(&self, grid: &mut CellGrid, layer: &WorldLayer, cam: &Camera) {
        // Slash trails: an arc of glyphs sweeping across.
        for s in &self.slashes {
            let k = s.t / 0.16;
            let base = s.dir.y.atan2(s.dir.x);
            let half = s.arc.to_radians() / 2.0;
            let color = if s.hero { Rgb(230, 240, 255) } else { Rgb(255, 110, 90) };
            let steps = 9;
            for i in 0..steps {
                let f = i as f32 / (steps - 1) as f32;
                if f > k * 1.4 {
                    continue;
                }
                let a = base - half + 2.0 * half * f;
                let r = s.range * 0.85;
                let p = Vec3::new(s.pos.x + a.cos() * r, 0.9, s.pos.y + a.sin() * r);
                let g = cam.world_to_grid(p);
                let ch = match ((a + std::f32::consts::FRAC_PI_4).rem_euclid(std::f32::consts::PI) / std::f32::consts::FRAC_PI_4) as i32 {
                    0 => '\\',
                    1 => '-',
                    2 => '/',
                    _ => '|',
                };
                put_depth(grid, layer, g, p.y, ch, color.scale(1.0 - k * 0.5));
            }
        }
        for p in &self.particles {
            let g = cam.world_to_grid(p.pos);
            let age = 1.0 - p.life / p.max;
            let glyphs: Vec<char> = p.glyphs.chars().collect();
            let ch = glyphs[((age * glyphs.len() as f32) as usize).min(glyphs.len() - 1)];
            let fade = (p.life / p.max).sqrt();
            put_depth(grid, layer, g, p.pos.y, ch, p.color.scale(0.35 + 0.65 * fade));
        }
        for t in &self.texts {
            let g = cam.world_to_grid(t.pos);
            let x = g.x as i32 - t.text.len() as i32 / 2;
            let fade = t.life.min(0.4) / 0.4;
            for (i, ch) in t.text.chars().enumerate() {
                grid.put(x + i as i32, g.y as i32, ch, t.color.scale(fade));
            }
        }
    }
}

fn put_depth(grid: &mut CellGrid, layer: &WorldLayer, g: Vec2, y: f32, ch: char, color: Rgb) {
    let (x, yy) = (g.x.floor() as i32, g.y.floor() as i32);
    if !grid.in_bounds(x, yy) {
        return;
    }
    let i = grid.idx(x, yy);
    if layer.height.get(i).copied().unwrap_or(f32::NEG_INFINITY) > y + 0.15 {
        return;
    }
    grid.cells[i] = Cell { ch, fg: color, bg: Rgb::BLACK };
}

/// Ground decal shapes (in world units on the floor).
#[derive(Clone, Copy, Debug)]
pub enum Decal {
    Circle { c: Vec2, r: f32 },
    Cone { c: Vec2, dir: Vec2, range: f32, arc: f32 },
    Line { from: Vec2, dir: Vec2, len: f32, width: f32 },
}

impl Decal {
    fn contains(&self, p: Vec2) -> bool {
        match *self {
            Decal::Circle { c, r } => p.distance(c) <= r,
            Decal::Cone { c, dir, range, arc } => {
                let d = p - c;
                let len = d.length();
                len <= range && (len < 0.3 || d.normalize().dot(dir) >= (arc.to_radians() / 2.0).cos())
            }
            Decal::Line { from, dir, len, width } => {
                let d = p - from;
                let t = d.dot(dir);
                t >= 0.0 && t <= len && (d - dir * t).length() <= width / 2.0
            }
        }
    }

    fn bounds(&self) -> (Vec2, Vec2) {
        match *self {
            Decal::Circle { c, r } => (c - Vec2::splat(r), c + Vec2::splat(r)),
            Decal::Cone { c, range, .. } => (c - Vec2::splat(range), c + Vec2::splat(range)),
            Decal::Line { from, dir, len, width } => {
                let to = from + dir * len;
                (from.min(to) - Vec2::splat(width), from.max(to) + Vec2::splat(width))
            }
        }
    }
}

/// Draw a decal on visible floor: outline in `color`, interior filled up to `progress` (0..1)
/// from the centre outwards (so a telegraph shows how long until it goes off).
pub fn draw_decal(grid: &mut CellGrid, layer: &WorldLayer, cam: &Camera, d: Decal, progress: f32, color: Rgb, fill: char) {
    let (lo, hi) = d.bounds();
    // Screen-space bounding box of the world-space bounds.
    let mut gmin = Vec2::splat(f32::INFINITY);
    let mut gmax = Vec2::splat(f32::NEG_INFINITY);
    for p in [Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y), Vec2::new(lo.x, hi.y), Vec2::new(hi.x, hi.y)] {
        let g = cam.world_to_grid(Vec3::new(p.x, 0.0, p.y));
        gmin = gmin.min(g);
        gmax = gmax.max(g);
    }
    let x0 = (gmin.x.floor() as i32 - 1).max(0);
    let y0 = (gmin.y.floor() as i32 - 1).max(0);
    let x1 = (gmax.x.ceil() as i32 + 1).min(grid.w as i32 - 1);
    let y1 = (gmax.y.ceil() as i32 + 1).min(grid.h as i32 - 1);
    let inside = |x: i32, y: i32| d.contains(cam.grid_to_plane(Vec2::new(x as f32 + 0.5, y as f32 + 0.5), 0.0));
    let (center, radius) = match d {
        Decal::Circle { c, r } => (c, r),
        Decal::Cone { c, range, .. } => (c, range),
        Decal::Line { from, len, .. } => (from, len),
    };
    for y in y0..=y1 {
        for x in x0..=x1 {
            if !inside(x, y) {
                continue;
            }
            let i = grid.idx(x, y);
            if !layer.floor.get(i).copied().unwrap_or(false) {
                continue;
            }
            let edge = !inside(x + 1, y) || !inside(x - 1, y) || !inside(x, y + 1) || !inside(x, y - 1);
            let p = cam.grid_to_plane(Vec2::new(x as f32 + 0.5, y as f32 + 0.5), 0.0);
            let k = p.distance(center) / radius.max(0.01);
            if edge {
                let ch = if y % 2 == 0 { '\'' } else { '.' };
                grid.cells[i] = Cell { ch, fg: color, bg: Rgb::BLACK };
            } else if k <= progress {
                grid.cells[i] = Cell { ch: fill, fg: color.scale(0.75), bg: Rgb::BLACK };
            } else {
                // Tint the floor glyph that's already there.
                let c = grid.cells[i];
                grid.cells[i] = Cell { ch: c.ch, fg: c.fg.lerp(color, 0.35), bg: Rgb::BLACK };
            }
        }
    }
}

/// Flickering fire glyphs over burning ground.
pub fn draw_flames(grid: &mut CellGrid, layer: &WorldLayer, cam: &Camera, c: Vec2, r: f32, time: f32) {
    let d = Decal::Circle { c, r };
    let (lo, hi) = d.bounds();
    let g0 = cam.world_to_grid(Vec3::new(lo.x, 0.0, hi.y));
    let g1 = cam.world_to_grid(Vec3::new(hi.x, 0.0, lo.y));
    let ga = cam.world_to_grid(Vec3::new(lo.x, 0.0, lo.y));
    let gb = cam.world_to_grid(Vec3::new(hi.x, 0.0, hi.y));
    let x0 = (g0.x.min(g1.x).floor() as i32).max(0);
    let x1 = (g0.x.max(g1.x).ceil() as i32).min(grid.w as i32 - 1);
    let y0 = (ga.y.floor() as i32).max(0);
    let y1 = (gb.y.ceil() as i32).min(grid.h as i32 - 1);
    let step = (time * 8.0) as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = cam.grid_to_plane(Vec2::new(x as f32 + 0.5, y as f32 + 0.5), 0.0);
            if p.distance(c) > r {
                continue;
            }
            let i = grid.idx(x, y);
            if !layer.floor.get(i).copied().unwrap_or(false) {
                continue;
            }
            let h = ((x * 7 + y * 13 + step * 5) % 9) as usize;
            let ch = ['^', '~', '*', '\'', '^', '.', '~', '^', ','][h];
            let col = if h.is_multiple_of(3) { FIRE } else { FIRE2 };
            grid.cells[i] = Cell { ch, fg: col, bg: Rgb::BLACK };
        }
    }
}
