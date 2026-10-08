//! Stamps: hand-drawn glyph grids for dungeon pieces (floors, walls, props).
//!
//! A stamp file holds one or more stamps:
//!
//! ```text
//! === wall_a
//! proxy: 0 0 1 1 10        // box footprint x0 z0 x1 z1 (0..1 of a tile) + height in rows
//! legend: s=wall_stone t=wall_top
//! fill: s                  // material for cells without a colours section
//! --- glyphs
//!       .--.
//!   ...
//! --- colors               // optional: one legend key per cell, space = transparent
//! ```
//!
//! The `proxy` box gives every cell a 3D position and facing (top / left / right), which is
//! used for lighting and for hiding 3D figures that walk behind the piece.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};

use super::iso::{screen_to_plane, ROWS_PER_Y, VIEW_DIR};
use super::material::MaterialLib;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Horizontal surface (floor or top of a block).
    Top,
    /// Faces +z (lower-left on screen).
    Left,
    /// Faces +x (lower-right on screen).
    Right,
    /// Decoration painted on the floor outside the proxy box.
    Flat,
}

impl Face {
    pub fn normal(self) -> Vec3 {
        match self {
            Face::Top | Face::Flat => Vec3::Y,
            Face::Left => Vec3::Z,
            Face::Right => Vec3::X,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StampCell {
    pub ch: char,
    pub mat: u16,
    pub face: Face,
    /// Position relative to the tile's corner (i, 0, j).
    pub pos: Vec3,
}

#[derive(Clone, Debug)]
pub enum Proxy {
    Floor,
    Box { x0: f32, z0: f32, x1: f32, z1: f32, rows: f32 },
}

#[derive(Clone, Debug)]
pub struct Stamp {
    pub name: String,
    pub w: i32,
    pub h: i32,
    /// Stamp cell that sits on the tile's top vertex.
    pub anchor: (i32, i32),
    pub cells: Vec<Option<StampCell>>,
    pub proxy: Proxy,
    /// Source text (for the gallery).
    pub file: String,
}

impl Stamp {
    pub fn cell(&self, c: i32, r: i32) -> Option<&StampCell> {
        if c < 0 || r < 0 || c >= self.w || r >= self.h {
            return None;
        }
        self.cells[(r * self.w + c) as usize].as_ref()
    }
}

#[derive(Clone, Debug, Default)]
pub struct StampLib {
    pub stamps: Vec<Stamp>,
    pub by_name: BTreeMap<String, usize>,
}

impl StampLib {
    pub fn get(&self, name: &str) -> Option<&Stamp> {
        self.by_name.get(name).map(|&i| &self.stamps[i])
    }

    pub fn id(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    pub fn add_file(&mut self, file: &str, text: &str, mats: &MaterialLib) -> Result<(), String> {
        for s in parse_stamps(file, text, mats)? {
            if self.by_name.contains_key(&s.name) {
                return Err(format!("{file}: duplicate stamp '{}'", s.name));
            }
            self.by_name.insert(s.name.clone(), self.stamps.len());
            self.stamps.push(s);
        }
        Ok(())
    }
}

/// Where does the view ray through screen offset (dc, dr) from the tile's top vertex hit the
/// proxy? Returns (face, position) or None if it misses.
pub fn proxy_hit(proxy: &Proxy, dc: f32, dr: f32) -> Option<(Face, Vec3)> {
    let g = screen_to_plane(Vec2::new(dc, dr), 0.0);
    match *proxy {
        Proxy::Floor => {
            if (0.0..=1.0).contains(&g.x) && (0.0..=1.0).contains(&g.y) {
                Some((Face::Top, Vec3::new(g.x, 0.0, g.y)))
            } else {
                None
            }
        }
        Proxy::Box { x0, z0, x1, z1, rows } => {
            let hy = rows / ROWS_PER_Y;
            let o = Vec3::new(g.x, 0.0, g.y);
            let d = -VIEW_DIR; // towards the camera
            let mut tmin = f32::NEG_INFINITY;
            let mut tmax = f32::INFINITY;
            for (p0, dv, lo, hi) in [(o.x, d.x, x0, x1), (o.y, d.y, 0.0, hy), (o.z, d.z, z0, z1)] {
                let t1 = (lo - p0) / dv;
                let t2 = (hi - p0) / dv;
                tmin = tmin.max(t1.min(t2));
                tmax = tmax.min(t1.max(t2));
            }
            if tmin > tmax {
                return None;
            }
            let p = o + d * tmax;
            let face = if (p.y - hy).abs() < 1e-4 {
                Face::Top
            } else if (p.x - x1).abs() < 1e-4 {
                Face::Right
            } else if (p.z - z1).abs() < 1e-4 {
                Face::Left
            } else {
                Face::Top
            };
            Some((face, p))
        }
    }
}

/// `=== name` starts a stamp. Strict so that art rows full of `=` are never mistaken for it.
fn stamp_header(line: &str) -> Option<&str> {
    let name = line.strip_prefix("=== ")?.trim();
    let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then_some(name)
}

/// `--- glyphs` / `--- colors` section markers.
fn section_header(line: &str) -> Option<&str> {
    let s = line.strip_prefix("--- ")?.trim();
    matches!(s, "glyphs" | "colors" | "colours").then_some(s)
}

fn parse_stamps(file: &str, text: &str, mats: &MaterialLib) -> Result<Vec<Stamp>, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if let Some(name) = stamp_header(l) {
            let start = i;
            i += 1;
            let mut j = i;
            while j < lines.len() && stamp_header(lines[j]).is_none() {
                j += 1;
            }
            let s = parse_one(file, name, &lines[i..j], start + 2, mats)?;
            out.push(s);
            i = j;
        } else {
            let t = l.trim();
            if !(t.is_empty() || t.starts_with('#') || t.starts_with("//")) {
                return Err(format!("{file}:{}: expected '=== name' to start a stamp", i + 1));
            }
            i += 1;
        }
    }
    Ok(out)
}

fn parse_one(file: &str, name: &str, lines: &[&str], first_line: usize, mats: &MaterialLib) -> Result<Stamp, String> {
    let err = |ln: usize, msg: String| format!("{file}:{}: stamp '{name}': {msg}", first_line + ln);
    let mut proxy = Proxy::Floor;
    let mut anchor: Option<(i32, i32)> = None;
    let mut legend: BTreeMap<char, u16> = BTreeMap::new();
    let mut fill: Option<u16> = None;
    let mut glyphs: Vec<&str> = Vec::new();
    let mut colors: Vec<&str> = Vec::new();
    let mut section = 0; // 0 header, 1 glyphs, 2 colors
    for (ln, l) in lines.iter().enumerate() {
        if let Some(sec) = section_header(l) {
            section = if sec == "glyphs" { 1 } else { 2 };
            continue;
        }
        match section {
            0 => {
                let t = l.trim();
                if t.is_empty() || t.starts_with("//") || t.starts_with('#') {
                    continue;
                }
                let (k, v) = t.split_once(':').ok_or_else(|| err(ln, "expected 'key: value'".into()))?;
                let v = v.split("//").next().unwrap_or("").trim();
                match k.trim() {
                    "proxy" => {
                        proxy = if v == "floor" {
                            Proxy::Floor
                        } else {
                            let n: Vec<f32> = v.split_whitespace().map(|x| x.parse::<f32>()).collect::<Result<_, _>>().map_err(|_| err(ln, "proxy needs 5 numbers".into()))?;
                            if n.len() != 5 {
                                return Err(err(ln, "proxy needs 'floor' or 'x0 z0 x1 z1 rows'".into()));
                            }
                            Proxy::Box { x0: n[0], z0: n[1], x1: n[2], z1: n[3], rows: n[4] }
                        };
                    }
                    "anchor" => {
                        let n: Vec<i32> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        if n.len() != 2 {
                            return Err(err(ln, "anchor needs 'col row'".into()));
                        }
                        anchor = Some((n[0], n[1]));
                    }
                    "legend" => {
                        for pair in v.split_whitespace() {
                            let (key, mat) = pair.split_once('=').ok_or_else(|| err(ln, format!("bad legend entry '{pair}'")))?;
                            let mut kc = key.chars();
                            let (Some(kch), None) = (kc.next(), kc.next()) else {
                                return Err(err(ln, format!("legend key '{key}' must be one character")));
                            };
                            let id = mats.id(mat).ok_or_else(|| err(ln, format!("unknown material '{mat}'")))?;
                            legend.insert(kch, id);
                        }
                    }
                    "fill" => {
                        let mut kc = v.chars();
                        let c = kc.next().ok_or_else(|| err(ln, "fill needs a legend key".into()))?;
                        fill = Some(*legend.get(&c).ok_or_else(|| err(ln, format!("fill key '{c}' is not in the legend (put legend first)")))?);
                    }
                    other => return Err(err(ln, format!("unknown key '{other}'"))),
                }
            }
            1 => glyphs.push(l),
            _ => colors.push(l),
        }
    }
    while glyphs.last().is_some_and(|l| l.trim().is_empty()) {
        glyphs.pop();
    }
    while colors.last().is_some_and(|l| l.trim().is_empty()) {
        colors.pop();
    }
    if glyphs.is_empty() {
        return Err(err(0, "missing '--- glyphs' section".into()));
    }
    let h = glyphs.len() as i32;
    let w = glyphs.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
    let rows = match proxy {
        Proxy::Floor => 0,
        Proxy::Box { rows, .. } => rows.round() as i32,
    };
    let anchor = anchor.unwrap_or((8, rows));
    let fill = fill.or_else(|| legend.values().next().copied()).unwrap_or(0);
    let gl: Vec<Vec<char>> = glyphs.iter().map(|l| l.chars().collect()).collect();
    let cl: Vec<Vec<char>> = colors.iter().map(|l| l.chars().collect()).collect();
    let mut cells = Vec::with_capacity((w * h) as usize);
    for r in 0..h {
        for c in 0..w {
            let ch = gl[r as usize].get(c as usize).copied().unwrap_or(' ');
            let dc = c as f32 + 0.5 - anchor.0 as f32;
            let dr = r as f32 + 0.5 - anchor.1 as f32;
            let hit = proxy_hit(&proxy, dc, dr);
            let mat = if !cl.is_empty() {
                let key = cl.get(r as usize).and_then(|row| row.get(c as usize)).copied().unwrap_or(' ');
                if key == ' ' {
                    None
                } else {
                    Some(*legend.get(&key).ok_or_else(|| err(0, format!("colour key '{key}' at row {} col {} is not in the legend", r + 1, c + 1)))?)
                }
            } else if hit.is_some() || ch != ' ' {
                Some(fill)
            } else {
                None
            };
            let cell = mat.map(|mat| {
                let (face, pos) = hit.unwrap_or_else(|| {
                    let g = screen_to_plane(Vec2::new(dc, dr), 0.0);
                    (Face::Flat, Vec3::new(g.x, 0.0, g.y))
                });
                StampCell { ch, mat, face, pos }
            });
            cells.push(cell);
        }
    }
    Ok(Stamp { name: name.to_string(), w, h, anchor, cells, proxy, file: file.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> MaterialLib {
        MaterialLib::parse(r##"{ "stone": (color: "#808080"), "top": (color: "#a0a0a0") }"##).unwrap()
    }

    #[test]
    fn wall_faces_follow_the_proxy() {
        let text = "=== w\nproxy: 0 0 1 1 10\nlegend: s=stone\n--- glyphs\n".to_string()
            + &"################\n".repeat(14);
        let s = &parse_stamps("t", &text, &mats()).unwrap()[0];
        assert_eq!((s.w, s.h, s.anchor), (16, 14, (8, 10)));
        assert_eq!(s.cell(7, 0).unwrap().face, Face::Top);
        assert_eq!(s.cell(0, 6).unwrap().face, Face::Left);
        assert_eq!(s.cell(15, 6).unwrap().face, Face::Right);
        // Front-bottom corner sits on the ground.
        assert!(s.cell(7, 13).unwrap().pos.y < 0.15);
        // Top is ~2 units high.
        assert!((s.cell(7, 0).unwrap().pos.y - 10.0 / ROWS_PER_Y).abs() < 1e-3);
    }

    #[test]
    fn floor_cells_outside_diamond_are_transparent() {
        let text = "=== f\nproxy: floor\nlegend: s=stone\n--- glyphs\n      ....\n  ............\n  ............\n      ....\n";
        let s = &parse_stamps("t", text, &mats()).unwrap()[0];
        assert!(s.cell(0, 0).is_none());
        assert!(s.cell(7, 0).is_some());
        assert_eq!(s.cell(7, 0).unwrap().face, Face::Top);
    }

    #[test]
    fn unknown_material_is_reported_with_line() {
        let text = "=== f\nlegend: s=nope\n--- glyphs\n.\n";
        let e = parse_stamps("file.stamp", text, &mats()).unwrap_err();
        assert!(e.contains("file.stamp:2") && e.contains("nope"), "{e}");
    }
}
