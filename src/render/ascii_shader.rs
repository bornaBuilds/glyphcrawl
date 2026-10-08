//! The ASCII shader: turns the 2×4 samples under each cell into one glyph.
//!
//! - Samples hidden behind dungeon geometry (height test against `WorldLayer`) are occluded.
//! - Cells where the figure's silhouette, a material boundary, a depth jump or a sharp crease
//!   runs through get an outline glyph chosen from the edge direction (`' - . _ / \ |`).
//! - Other covered cells get a glyph from the material's ramp by how brightly they're lit.
//! - Occluded silhouettes of heroes/enemies are drawn as a coloured "x-ray" outline.

use glam::{Vec2, Vec3};
use serde::Deserialize;

use super::cell::{Cell, CellGrid, Rgb};
use super::iso::VIEW_DIR;
use super::material::MaterialLib;
use super::raycast::{SampleBuffer, SX, SY};
use super::world::{shade, tone_rgb, LightSrc, WorldLayer};

/// Tunable shader parameters (`assets/shader.ron`).
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    /// Depth jump (world units) that counts as an edge.
    pub edge_depth: f32,
    /// Normals whose dot product is below this count as a crease.
    pub edge_normal: f32,
    /// Material changes inside a figure draw an outline.
    pub edge_material: bool,
    /// Minimum edge samples in a cell to draw an outline glyph.
    pub edge_min: u32,
    /// Minimum covered samples (of 8) for a figure to take over a cell.
    pub coverage_min: u32,
    /// Fixed key light so figures always read: direction towards the light, and strength.
    pub key_dir: (f32, f32, f32),
    pub key: f32,
    pub ambient: f32,
    pub rim: f32,
    /// How much lower than the dungeon surface a sample must be to count as hidden.
    pub occlusion_bias: f32,
    /// Below this sub-row average a horizontal edge uses `'`, then `-`, then `.`, then `_`.
    pub shallow_angle: f32,
    pub steep_angle: f32,
}

impl Default for ShaderParams {
    fn default() -> Self {
        ShaderParams {
            edge_depth: 0.6,
            edge_normal: -0.2,
            edge_material: false,
            edge_min: 3,
            coverage_min: 3,
            key_dir: (-0.35, 0.8, 0.5),
            key: 0.55,
            ambient: 0.18,
            rim: 0.25,
            occlusion_bias: 0.08,
            shallow_angle: 35.0,
            steep_angle: 68.0,
        }
    }
}

/// How each figure in this frame should be drawn (indexed by owner - 1).
#[derive(Clone, Copy, Debug, Default)]
pub struct FigureStyle {
    /// > 0 draws the figure white (hit flash).
    pub flash: f32,
    /// Colour of the outline shown through walls (None = no x-ray).
    pub xray: Option<Rgb>,
    /// Override outline colour (hover highlight).
    pub edge_tint: Option<Rgb>,
    /// Multiply all colours (e.g. frozen = blue-ish).
    pub tint: Option<Rgb>,
    /// Tile index for light masks (so torches in other rooms don't light this figure).
    pub tile: Option<usize>,
    /// 0..1 — fades the figure to sparse glyphs (dying / invisible).
    pub fade: f32,
}

fn edge_glyph(theta_deg: f32, sub_row: f32, p: &ShaderParams) -> char {
    // theta: tangent direction in sample space (y down), folded into (-90, 90].
    let a = theta_deg.abs();
    if a < p.shallow_angle {
        if sub_row < 1.0 {
            '\''
        } else if sub_row < 2.0 {
            '-'
        } else if sub_row < 3.0 {
            '.'
        } else {
            '_'
        }
    } else if a < p.steep_angle {
        if theta_deg > 0.0 { '\\' } else { '/' }
    } else {
        '|'
    }
}

/// Orientation of the edge through a set of samples, via the structure tensor of the
/// gradient of `field` (robust for thin features where gradients cancel).
fn edge_orientation(field: impl Fn(i32, i32) -> f32, pts: &[(i32, i32)]) -> f32 {
    let (mut gxx, mut gyy, mut gxy) = (0.0f32, 0.0f32, 0.0f32);
    for &(x, y) in pts {
        let f = |dx: i32, dy: i32| field(x + dx, y + dy);
        let gx = (f(1, -1) + 2.0 * f(1, 0) + f(1, 1)) - (f(-1, -1) + 2.0 * f(-1, 0) + f(-1, 1));
        let gy = (f(-1, 1) + 2.0 * f(0, 1) + f(1, 1)) - (f(-1, -1) + 2.0 * f(0, -1) + f(1, -1));
        gxx += gx * gx;
        gyy += gy * gy;
        gxy += gx * gy;
    }
    // Dominant gradient direction; the edge tangent is perpendicular to it.
    let grad = 0.5 * (2.0 * gxy).atan2(gxx - gyy);
    let mut t = (grad + std::f32::consts::FRAC_PI_2).to_degrees();
    while t > 90.0 {
        t -= 180.0;
    }
    while t <= -90.0 {
        t += 180.0;
    }
    t
}

pub struct ResolveCtx<'a> {
    pub layer: &'a WorldLayer,
    pub mats: &'a MaterialLib,
    pub lights: &'a [LightSrc<'a>],
    pub params: &'a ShaderParams,
    pub styles: &'a [FigureStyle],
}

/// Occlusion test against the dungeon, then turn samples into glyphs.
pub fn resolve(grid: &mut CellGrid, buf: &mut SampleBuffer, rc: &ResolveCtx) {
    let Some((x0, y0, x1, y1)) = buf.dirty else { return };
    let p = rc.params;
    // 1. Occlusion.
    for sy in y0..y1 {
        for sx in x0..x1 {
            let i = sy * buf.w + sx;
            if buf.owner[i] == 0 {
                continue;
            }
            let (cx, cy) = (sx / SX, sy / SY);
            if cx >= rc.layer.w as usize || cy >= rc.layer.h as usize {
                continue;
            }
            let ci = cy * rc.layer.w as usize + cx;
            let frac = Vec2::new(((sx % SX) as f32 + 0.5) / SX as f32, ((sy % SY) as f32 + 0.5) / SY as f32);
            let h = rc.layer.height_at(ci, frac);
            buf.occ[i] = buf.y[i] < h - p.occlusion_bias;
        }
    }

    let w = buf.w as i32;
    let h = buf.h as i32;
    let vis = |x: i32, y: i32| -> u16 {
        if x < 0 || y < 0 || x >= w || y >= h {
            return 0;
        }
        let i = (y * w + x) as usize;
        if buf.occ[i] { 0 } else { buf.owner[i] }
    };
    let hidden = |x: i32, y: i32| -> u16 {
        if x < 0 || y < 0 || x >= w || y >= h {
            return 0;
        }
        let i = (y * w + x) as usize;
        if buf.occ[i] { buf.owner[i] } else { 0 }
    };
    let key_dir = Vec3::new(p.key_dir.0, p.key_dir.1, p.key_dir.2).normalize();
    let to_cam = -VIEW_DIR;

    let (cx0, cy0) = (x0 / SX, y0 / SY);
    let (cx1, cy1) = (x1.div_ceil(SX).min(grid.w as usize), y1.div_ceil(SY).min(grid.h as usize));
    let mut pts: Vec<(i32, i32)> = Vec::with_capacity(8);
    for cy in cy0..cy1 {
        for cx in cx0..cx1 {
            // Majority visible owner in this cell.
            let mut counts: [(u16, u32); 8] = [(0, 0); 8];
            let mut hidden_any = 0u16;
            for k in 0..SX * SY {
                let sx = (cx * SX + k % SX) as i32;
                let sy = (cy * SY + k / SX) as i32;
                let o = vis(sx, sy);
                if o != 0 {
                    if let Some(e) = counts.iter_mut().find(|e| e.0 == o || e.0 == 0) {
                        e.0 = o;
                        e.1 += 1;
                    }
                } else if hidden_any == 0 {
                    hidden_any = hidden(sx, sy);
                }
            }
            let (owner, cover) = counts.iter().copied().max_by_key(|e| e.1).unwrap_or((0, 0));
            if owner == 0 || cover < p.coverage_min.clamp(1, 2) {
                // X-ray outline for hidden heroes/enemies.
                if hidden_any != 0 && cover == 0 {
                    let style = rc.styles.get(hidden_any as usize - 1).copied().unwrap_or_default();
                    if let Some(xc) = style.xray {
                        pts.clear();
                        let mut rows = 0.0;
                        for k in 0..SX * SY {
                            let sx = (cx * SX + k % SX) as i32;
                            let sy = (cy * SY + k / SX) as i32;
                            if hidden(sx, sy) == hidden_any {
                                let n4 = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                                if n4.iter().any(|(dx, dy)| hidden(sx + dx, sy + dy) != hidden_any && vis(sx + dx, sy + dy) != hidden_any) {
                                    pts.push((sx, sy));
                                    rows += (k / SX) as f32;
                                }
                            }
                        }
                        if pts.len() >= 2 {
                            let field = |x: i32, y: i32| if hidden(x, y) == hidden_any || vis(x, y) == hidden_any { 1.0 } else { 0.0 };
                            let th = edge_orientation(field, &pts);
                            let ch = edge_glyph(th, rows / pts.len() as f32, p);
                            grid.set(cx as i32, cy as i32, Cell { ch, fg: xc, bg: Rgb::BLACK });
                        }
                    }
                }
                continue;
            }
            let style = rc.styles.get(owner as usize - 1).copied().unwrap_or_default();
            if cover < p.coverage_min {
                // Thin features (sword tips) only show over empty space.
                let under = grid.get(cx as i32, cy as i32).map(|c| c.ch).unwrap_or(' ');
                if under != ' ' && under != '.' {
                    continue;
                }
            }

            // Edge samples.
            pts.clear();
            let mut rows = 0.0;
            let mut n_sum = Vec3::ZERO;
            let mut p_sum = Vec3::ZERO;
            let mut mat_count: [(u16, u32); 8] = [(u16::MAX, 0); 8];
            for k in 0..SX * SY {
                let sx = (cx * SX + k % SX) as i32;
                let sy = (cy * SY + k / SX) as i32;
                if vis(sx, sy) != owner {
                    continue;
                }
                let i = (sy * w + sx) as usize;
                n_sum += buf.n[i];
                p_sum += buf.p[i];
                if let Some(e) = mat_count.iter_mut().find(|e| e.0 == buf.mat[i] || e.0 == u16::MAX) {
                    e.0 = buf.mat[i];
                    e.1 += 1;
                }
                let mut edge = false;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (sx + dx, sy + dy);
                    let same_cell = nx.div_euclid(SX as i32) == cx as i32 && ny.div_euclid(SY as i32) == cy as i32;
                    if vis(nx, ny) != owner {
                        // Silhouettes only count where the outline passes through this cell.
                        if same_cell {
                            edge = true;
                            break;
                        }
                        continue;
                    }
                    let j = (ny * w + nx) as usize;
                    if (p.edge_material && buf.mat[j] != buf.mat[i])
                        || (buf.y[j] - buf.y[i]).abs() > p.edge_depth
                        || buf.n[j].dot(buf.n[i]) < p.edge_normal
                    {
                        edge = true;
                        break;
                    }
                }
                if edge {
                    pts.push((sx, sy));
                    rows += (k / SX) as f32;
                }
            }
            let mat_id = mat_count.iter().copied().filter(|e| e.0 != u16::MAX).max_by_key(|e| e.1).map(|e| e.0).unwrap_or(0);
            let mat = rc.mats.get(mat_id);
            let n = n_sum.normalize_or_zero();
            let pos = p_sum / cover as f32;

            // Lighting: fixed key light + ambient + rim + dungeon lights.
            let ndl = n.dot(key_dir).max(0.0);
            let rim = (1.0 - n.dot(to_cam).max(0.0)).powi(2) * p.rim;
            let base = p.ambient + p.key * ndl + rim;
            let pl = shade(pos, n, style.tile, rc.lights, [0.0; 3]);
            let light = [base + pl[0], base + pl[1] * 0.95, base + pl[2] * 0.9];
            let intensity = ((light[0] + light[1] + light[2]) / 3.0).clamp(0.0, 1.6) / 1.6;

            let mut fg = if mat.emissive > 0.0 {
                tone_rgb(mat.color, light).lerp(mat.color, mat.emissive)
            } else {
                tone_rgb(mat.color, light)
            };
            let is_edge = pts.len() as u32 >= p.edge_min;
            let ch = if is_edge {
                let field = |x: i32, y: i32| {
                    let o = vis(x, y);
                    if o != owner {
                        return 0.0;
                    }
                    let i = (y * w + x) as usize;
                    1.0 + 0.6 * buf.y[i] + if p.edge_material { buf.mat[i] as f32 * 0.37 } else { 0.0 }
                };
                let th = edge_orientation(field, &pts);
                fg = match style.edge_tint {
                    Some(t) => t,
                    None => tone_rgb(mat.edge, [light[0].max(0.6), light[1].max(0.6), light[2].max(0.6)]),
                };
                edge_glyph(th, rows / pts.len() as f32, p)
            } else {
                let ramp = &mat.ramp;
                let v = if mat.emissive > 0.0 { intensity.max(mat.emissive) } else { intensity };
                let idx = ((v.powf(0.8) * (ramp.len() - 1) as f32).round() as usize).clamp(1, ramp.len() - 1);
                ramp[idx]
            };
            if let Some(t) = style.tint {
                fg = Rgb(
                    (fg.0 as u16 * t.0 as u16 / 255) as u8,
                    (fg.1 as u16 * t.1 as u16 / 255) as u8,
                    (fg.2 as u16 * t.2 as u16 / 255) as u8,
                );
            }
            if style.flash > 0.0 {
                fg = fg.lerp(Rgb::WHITE, style.flash.min(1.0));
            }
            let ch = if style.fade > 0.0 {
                // Dissolve: drop cells in a stable dither pattern.
                let hsh = ((cx * 7 + cy * 13) % 10) as f32 / 10.0;
                if hsh < style.fade {
                    continue;
                }
                ch
            } else {
                ch
            };
            grid.set(cx as i32, cy as i32, Cell { ch, fg, bg: Rgb::BLACK });
        }
    }
}
