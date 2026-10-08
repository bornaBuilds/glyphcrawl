//! Loading and parsing everything under `assets/`. Every error names the file (and line
//! where possible); a failed F5 reload keeps the previous assets.

use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use crate::game::level::RoomTemplate;
use crate::render::ascii_shader::ShaderParams;
use crate::render::figure::{Figure, FigureFile};
use crate::render::material::{GlyphDim, MaterialLib};
use crate::render::stamp::StampLib;
use crate::render::world::{Tileset, TilesetDef, WorldArt};

#[derive(Clone, Debug, Default)]
pub struct Assets {
    pub font: Vec<u8>,
    pub mats: MaterialLib,
    pub stamps: StampLib,
    pub tileset: Tileset,
    pub dim: GlyphDim,
    pub rooms: Vec<RoomTemplate>,
    pub figures: BTreeMap<String, Figure>,
    pub shader: ShaderParams,
    pub title: String,
}

/// Parse RON with `Option` fields allowed to be written without `Some(...)`.
pub fn parse_ron<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
        .map_err(|e| e.to_string())
}

fn read(dir: &Path, rel: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(dir.join(rel)).map_err(|e| anyhow::anyhow!("{rel}: {e}"))
}

/// Sorted list of files with an extension in a sub-directory.
fn list(dir: &Path, sub: &str, ext: &str) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let d = dir.join(sub);
    for e in std::fs::read_dir(&d).map_err(|e| anyhow::anyhow!("{sub}/: {e}"))? {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) == Some(ext) {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            out.push((name, p));
        }
    }
    out.sort();
    Ok(out)
}

impl Assets {
    pub fn load(dir: &Path) -> anyhow::Result<Assets> {
        let font = std::fs::read(dir.join("fonts/DejaVuSansMono.ttf"))
            .map_err(|e| anyhow::anyhow!("fonts/DejaVuSansMono.ttf: {e}"))?;
        let mats = MaterialLib::parse(&read(dir, "materials.ron")?).map_err(|e| anyhow::anyhow!("materials.ron: {e}"))?;
        let dim = GlyphDim::parse(&read(dir, "glyph_dim.ron")?).map_err(|e| anyhow::anyhow!("glyph_dim.ron: {e}"))?;

        let mut stamps = StampLib::default();
        for (name, path) in list(dir, "stamps", "stamp")? {
            let text = std::fs::read_to_string(&path)?;
            stamps.add_file(&format!("stamps/{name}.stamp"), &text, &mats).map_err(|e| anyhow::anyhow!(e))?;
        }
        let tdef: TilesetDef = parse_ron(&read(dir, "tileset.ron")?).map_err(|e| anyhow::anyhow!("tileset.ron: {e}"))?;
        let tileset = Tileset::resolve(&tdef, &stamps).map_err(|e| anyhow::anyhow!(e))?;

        let mut rooms = Vec::new();
        for (name, path) in list(dir, "rooms", "room")? {
            let text = std::fs::read_to_string(&path)?;
            let t = RoomTemplate::parse(&name, &text).map_err(|e| anyhow::anyhow!("rooms/{name}.room: {e}"))?;
            rooms.push(t);
        }
        let mut figures = BTreeMap::new();
        for (name, path) in list(dir, "figures", "ron")? {
            let text = std::fs::read_to_string(&path)?;
            let file: FigureFile = parse_ron(&text).map_err(|e| anyhow::anyhow!("figures/{name}.ron: {e}"))?;
            let fig = Figure::from_file(&name, file, |m| mats.id(m)).map_err(|e| anyhow::anyhow!("figures/{name}.ron: {e}"))?;
            figures.insert(name, fig);
        }
        let shader: ShaderParams = parse_ron(&read(dir, "shader.ron")?).map_err(|e| anyhow::anyhow!("shader.ron: {e}"))?;
        let title = read(dir, "title.txt").unwrap_or_else(|_| "GLYPHCRAWL".into());

        Ok(Assets { font, mats, stamps, tileset, dim, rooms, figures, shader, title })
    }

    pub fn world_art(&self) -> WorldArt<'_> {
        WorldArt { mats: &self.mats, stamps: &self.stamps, tileset: &self.tileset, dim: &self.dim }
    }

    pub fn room(&self, name: &str) -> Option<usize> {
        self.rooms.iter().position(|r| r.name == name)
    }
}

/// Find the assets directory: explicit flag, then the source tree, then next to the binary.
pub fn find_dir(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p.to_path_buf());
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            candidates.push(d.join("assets"));
        }
    }
    candidates.push(PathBuf::from("assets"));
    candidates
        .into_iter()
        .find(|p| p.join("materials.ron").is_file())
        .ok_or_else(|| anyhow::anyhow!("could not find the assets/ directory (use --assets DIR)"))
}
