//! Player settings, persisted to `~/.config/glyphcrawl/settings.ron`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMode {
    /// WASD if the terminal reports key releases, otherwise mouse-only.
    Auto,
    MouseOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorPref {
    Auto,
    TrueColor,
    Ansi256,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameCap {
    Auto,
    Fps30,
    Fps60,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub show_fps: bool,
    pub input_mode: InputMode,
    pub color: ColorPref,
    pub screen_shake: bool,
    pub frame_cap: FrameCap,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 0.8,
            show_fps: false,
            input_mode: InputMode::Auto,
            color: ColorPref::Auto,
            screen_shake: true,
            frame_cap: FrameCap::Auto,
        }
    }
}

fn path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "glyphcrawl").map(|d| d.config_dir().join("settings.ron"))
}

impl Settings {
    pub fn load() -> Settings {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| ron::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(p) = path() else { return Ok(()) };
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        std::fs::write(p, s)?;
        Ok(())
    }
}
