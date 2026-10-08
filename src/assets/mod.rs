//! Loading and parsing everything under `assets/`.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Assets {
    pub font: Vec<u8>,
}

impl Assets {
    pub fn load(dir: &Path) -> anyhow::Result<Assets> {
        let font = std::fs::read(dir.join("fonts/DejaVuSansMono.ttf"))
            .map_err(|e| anyhow::anyhow!("fonts/DejaVuSansMono.ttf: {e}"))?;
        Ok(Assets { font })
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
        .find(|p| p.join("fonts").is_dir())
        .ok_or_else(|| anyhow::anyhow!("could not find the assets/ directory (use --assets DIR)"))
}
