//! Materials: a colour plus the glyph ramp the ASCII shader uses for it.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::cell::Rgb;

pub const DEFAULT_RAMP: &str = " .:-=+*#%@";

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub color: Rgb,
    /// Colour used for outline glyphs (`/ \ | _`) on 3D figures.
    pub edge: Rgb,
    /// Glyphs from darkest to brightest.
    pub ramp: Vec<char>,
    /// 0 = lit normally, 1 = glows at full brightness regardless of light.
    pub emissive: f32,
}

#[derive(Deserialize)]
struct MaterialDef {
    color: String,
    #[serde(default)]
    edge: Option<String>,
    #[serde(default)]
    ramp: Option<String>,
    #[serde(default)]
    emissive: f32,
}

#[derive(Clone, Debug, Default)]
pub struct MaterialLib {
    pub mats: Vec<Material>,
    pub by_name: BTreeMap<String, u16>,
}

impl MaterialLib {
    pub fn parse(text: &str) -> Result<MaterialLib, String> {
        let defs: BTreeMap<String, MaterialDef> = crate::assets::parse_ron(text)?;
        let mut lib = MaterialLib::default();
        // Index 0 is a fallback magenta so missing materials are obvious.
        lib.push(Material {
            name: "missing".into(),
            color: Rgb(255, 0, 255),
            edge: Rgb(255, 120, 255),
            ramp: DEFAULT_RAMP.chars().collect(),
            emissive: 0.0,
        });
        for (name, d) in defs {
            let color = Rgb::from_hex(&d.color).ok_or_else(|| format!("material '{name}': bad colour '{}'", d.color))?;
            let edge = match d.edge {
                Some(e) => Rgb::from_hex(&e).ok_or_else(|| format!("material '{name}': bad edge colour '{e}'"))?,
                None => color.lerp(Rgb::WHITE, 0.35),
            };
            let ramp: Vec<char> = d.ramp.as_deref().unwrap_or(DEFAULT_RAMP).chars().collect();
            if ramp.len() < 2 {
                return Err(format!("material '{name}': ramp needs at least 2 glyphs"));
            }
            lib.push(Material { name: name.clone(), color, edge, ramp, emissive: d.emissive });
        }
        Ok(lib)
    }

    fn push(&mut self, m: Material) {
        self.by_name.insert(m.name.clone(), self.mats.len() as u16);
        self.mats.push(m);
    }

    pub fn id(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    /// Material id, or the magenta fallback.
    pub fn id_or_missing(&self, name: &str) -> u16 {
        self.id(name).unwrap_or(0)
    }

    pub fn get(&self, id: u16) -> &Material {
        self.mats.get(id as usize).unwrap_or(&self.mats[0])
    }
}

/// Glyph dimming chains: in the dark, glyphs step towards sparser ones (`# -> = -> - -> .`).
#[derive(Clone, Debug, Default)]
pub struct GlyphDim {
    next: BTreeMap<char, char>,
}

impl GlyphDim {
    pub fn parse(text: &str) -> Result<GlyphDim, String> {
        let chains: Vec<String> = crate::assets::parse_ron(text)?;
        let mut next = BTreeMap::new();
        for chain in chains {
            let cs: Vec<char> = chain.chars().collect();
            for w in cs.windows(2) {
                next.entry(w[0]).or_insert(w[1]);
            }
        }
        Ok(GlyphDim { next })
    }

    pub fn dim(&self, mut ch: char, steps: u32) -> char {
        for _ in 0..steps {
            ch = match self.next.get(&ch) {
                Some(n) => *n,
                None => {
                    // Unknown glyphs fade straight to a dot, then nothing.
                    if ch == '.' || ch == ' ' { ' ' } else { '.' }
                }
            };
        }
        ch
    }
}
