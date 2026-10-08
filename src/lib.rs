//! glyphcrawl — an isometric ASCII dungeon crawler.
//!
//! Layers (top depends on bottom):
//! - `app`      — main loop, screens (menus, game, gallery), settings, bot
//! - `render`   — CellGrid, iso maths, stamps, 3D raycaster, ASCII shader, UI widgets
//! - `assets`   — loading/parsing of every file under `assets/`
//! - `game`     — pure simulation: no I/O, no clocks (checked by `tests/architecture.rs`)
//! - `platform` — terminal / window / headless backends + audio, all behind `Backend`

pub mod app;
pub mod assets;
pub mod game;
pub mod platform;
pub mod render;
