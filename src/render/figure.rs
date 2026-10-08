//! 3D figures made of primitive parts (box, sphere, cylinder, cone, octahedron), defined in
//! `assets/figures/*.ron` and animated by moving their parts.
//!
//! Figure space: +x = forward (the way the actor faces), +y = up, +z = the figure's right.
//! Part rotations are in degrees: `z` swings forward/back (like an arm), `x` tilts sideways,
//! `y` turns.

use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Shape {
    Box,
    Sphere,
    Cylinder,
    Cone,
    Octahedron,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PartDef {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    pub shape: Shape,
    /// Size of the shape (box: width/height/depth; sphere: diameters).
    pub size: (f32, f32, f32),
    /// Pivot position in the parent's frame (or the figure's root frame).
    #[serde(default)]
    pub at: (f32, f32, f32),
    /// Offset of the shape's centre from the pivot (e.g. an arm hangs below its shoulder).
    #[serde(default)]
    pub center: (f32, f32, f32),
    /// Rest rotation in degrees.
    #[serde(default)]
    pub rot: (f32, f32, f32),
    pub mat: String,
    /// Hidden parts still act as pivots for their children.
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Channel {
    Rot,
    Pos,
    Scale,
}

#[derive(Clone, Debug, Deserialize)]
pub enum Track {
    /// Keyframes (time, value), linearly interpolated.
    Keys { part: String, ch: Channel, keys: Vec<(f32, (f32, f32, f32))> },
    /// value = axis * amp * sin(2π (freq·t + phase))
    Osc { part: String, ch: Channel, axis: (f32, f32, f32), amp: f32, freq: f32, #[serde(default)] phase: f32 },
}

#[derive(Clone, Debug, Deserialize)]
pub struct AnimClip {
    pub dur: f32,
    #[serde(default)]
    pub looped: bool,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FigureFile {
    #[serde(default = "one")]
    pub scale: f32,
    pub parts: Vec<PartDef>,
    #[serde(default)]
    pub clips: BTreeMap<String, AnimClip>,
}

fn one() -> f32 {
    1.0
}

/// A loaded figure with parent links resolved and materials looked up.
#[derive(Clone, Debug)]
pub struct Figure {
    pub name: String,
    pub scale: f32,
    pub parts: Vec<PartDef>,
    pub parent: Vec<Option<usize>>,
    pub mat: Vec<u16>,
    pub clips: BTreeMap<String, AnimClip>,
    /// Track part names resolved to indices, per clip.
    pub clip_parts: BTreeMap<String, Vec<usize>>,
}

impl Figure {
    pub fn from_file(name: &str, f: FigureFile, mat_id: impl Fn(&str) -> Option<u16>) -> Result<Figure, String> {
        let mut index = BTreeMap::new();
        for (i, p) in f.parts.iter().enumerate() {
            if index.insert(p.name.clone(), i).is_some() {
                return Err(format!("duplicate part '{}'", p.name));
            }
        }
        let mut parent = Vec::new();
        let mut mat = Vec::new();
        for (i, p) in f.parts.iter().enumerate() {
            let par = match &p.parent {
                Some(n) => {
                    let j = *index.get(n).ok_or_else(|| format!("part '{}': unknown parent '{n}'", p.name))?;
                    if j >= i {
                        return Err(format!("part '{}': parent '{n}' must be listed before it", p.name));
                    }
                    Some(j)
                }
                None => None,
            };
            parent.push(par);
            mat.push(mat_id(&p.mat).ok_or_else(|| format!("part '{}': unknown material '{}'", p.name, p.mat))?);
        }
        let mut clip_parts = BTreeMap::new();
        for (cname, clip) in &f.clips {
            let mut v = Vec::new();
            for t in &clip.tracks {
                let pn = match t {
                    Track::Keys { part, .. } | Track::Osc { part, .. } => part,
                };
                v.push(*index.get(pn).ok_or_else(|| format!("clip '{cname}': unknown part '{pn}'"))?);
            }
            clip_parts.insert(cname.clone(), v);
        }
        Ok(Figure { name: name.to_string(), scale: f.scale, parts: f.parts, parent, mat, clips: f.clips, clip_parts })
    }
}

/// One clip playing at time `t` with weight `w` (0..1, for fading out).
#[derive(Clone, Copy, Debug)]
pub struct ClipPlay<'a> {
    pub name: &'a str,
    pub t: f32,
    pub w: f32,
}

fn v3(t: (f32, f32, f32)) -> Vec3 {
    Vec3::new(t.0, t.1, t.2)
}

fn sample_track(track: &Track, t: f32) -> (Channel, Vec3) {
    match track {
        Track::Keys { ch, keys, .. } => {
            if keys.is_empty() {
                return (*ch, Vec3::ZERO);
            }
            if t <= keys[0].0 {
                return (*ch, v3(keys[0].1));
            }
            for w in keys.windows(2) {
                let (t0, a) = (w[0].0, v3(w[0].1));
                let (t1, b) = (w[1].0, v3(w[1].1));
                if t <= t1 {
                    let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                    // Smoothstep easing reads better than linear for swings.
                    let k = k * k * (3.0 - 2.0 * k);
                    return (*ch, a.lerp(b, k));
                }
            }
            (*ch, v3(keys[keys.len() - 1].1))
        }
        Track::Osc { ch, axis, amp, freq, phase, .. } => {
            let s = (std::f32::consts::TAU * (freq * t + phase)).sin();
            (*ch, v3(*axis) * amp * s)
        }
    }
}

/// World transform of each part's unit primitive, plus the pivot frames.
pub struct Pose {
    /// Unit primitive -> world.
    pub shape: Vec<Mat4>,
}

/// Compute the pose of a figure at world position `pos` facing `facing` radians.
/// `squash` scales the whole figure (y, xz) for squash-and-stretch.
pub fn pose(fig: &Figure, pos: Vec3, facing: f32, scale: f32, squash: Vec3, clips: &[ClipPlay]) -> Pose {
    let n = fig.parts.len();
    let mut rot_add = vec![Vec3::ZERO; n];
    let mut pos_add = vec![Vec3::ZERO; n];
    let mut scale_mul = vec![Vec3::ONE; n];
    for cp in clips {
        let (Some(clip), Some(idx)) = (fig.clips.get(cp.name), fig.clip_parts.get(cp.name)) else { continue };
        let t = if clip.looped && clip.dur > 0.0 { cp.t.rem_euclid(clip.dur) } else { cp.t.min(clip.dur) };
        for (track, &pi) in clip.tracks.iter().zip(idx) {
            let (ch, v) = sample_track(track, t);
            let v = v * cp.w;
            match ch {
                Channel::Rot => rot_add[pi] += v,
                Channel::Pos => pos_add[pi] += v,
                Channel::Scale => scale_mul[pi] *= Vec3::ONE + v,
            }
        }
    }
    let root = Mat4::from_translation(pos)
        * Mat4::from_rotation_y(-facing)
        * Mat4::from_scale(squash * scale * fig.scale);
    let mut frame = vec![Mat4::IDENTITY; n];
    let mut shape = vec![Mat4::IDENTITY; n];
    for i in 0..n {
        let p = &fig.parts[i];
        let r = v3(p.rot) + rot_add[i];
        let q = Quat::from_euler(EulerRot::YZX, r.y.to_radians(), r.z.to_radians(), r.x.to_radians());
        let local = Mat4::from_translation(v3(p.at) + pos_add[i]) * Mat4::from_quat(q);
        frame[i] = match fig.parent[i] {
            Some(j) => frame[j] * local,
            None => root * local,
        };
        shape[i] = frame[i] * Mat4::from_translation(v3(p.center)) * Mat4::from_scale(v3(p.size) * scale_mul[i]);
    }
    Pose { shape }
}
