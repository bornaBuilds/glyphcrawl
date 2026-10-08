//! Same seed + same inputs = same game, tick for tick.

use glam::Vec2;
use glyphcrawl::assets::Assets;
use glyphcrawl::game::levelgen::generate;
use glyphcrawl::game::world::{TickInput, World};

fn assets() -> Assets {
    Assets::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")).expect("assets load")
}

/// A scripted player: walk in a slow circle and swing every so often.
fn script(tick: u64, hero: Vec2) -> TickInput {
    let a = tick as f32 * 0.01;
    let mut ti = TickInput { move_dir: Vec2::new(a.cos(), a.sin()), aim: hero + Vec2::new(a.sin(), a.cos()) * 3.0, ..Default::default() };
    if tick % 30 == 0 {
        ti.act_pressed = [true, false, tick % 90 == 0, false];
    }
    ti
}

fn run(seed: u64, class: &str, ticks: u64) -> u64 {
    let a = assets();
    let level = generate(&a.rooms, seed).unwrap();
    let mut w = World::new(level, &a.defs, class, seed);
    for t in 0..ticks {
        let ti = script(t, w.hero().pos);
        w.step(&a.defs, &ti, 1.0 / 60.0);
        w.events.clear();
    }
    w.state_hash()
}

#[test]
fn same_seed_same_world() {
    for class in ["knight", "wizard", "rogue", "archer"] {
        assert_eq!(run(42, class, 3600), run(42, class, 3600), "{class} diverged");
    }
}

#[test]
fn different_seeds_differ() {
    assert_ne!(run(1, "knight", 600), run(2, "knight", 600));
}

#[test]
fn every_asset_file_loads() {
    let a = assets();
    assert_eq!(a.defs.classes.len(), 4);
    assert!(a.defs.enemies.values().any(|e| e.boss));
    assert!(a.figures.len() >= 13);
}
