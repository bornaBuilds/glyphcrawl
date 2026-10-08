# glyphcrawl — project spec

Read this first in a new session. It says what the game is, what has been built (and when),
how the code is organised, the decisions that are settled, and what is still unverified.
For player-facing docs (controls, CLI flags, how to author art) see `README.md`.

---

## 1. What it is

A 2.5D isometric, real-time roguelike dungeon crawler drawn entirely in ASCII.

- **The twist:** the dungeon is hand-drawn ASCII "stamps"; heroes, monsters, projectiles and
  pickups are **live 3D figures made of primitives** (box, sphere, cylinder, cone, octahedron),
  raycast on the CPU and turned into characters by an **ASCII shader**.
- **Runs in a terminal** (crossterm) **or a native window** (minifb + fontdue). Same game code.
- **Inspirations:** Diablo (controls), Legend of Grimrock (dark exploration), Hades (sealed rooms,
  pick 1 of 3 upgrades).
- **Demo scope:** main menu → pick one of 4 heroes → one generated dungeon (start room, 4–6
  combat rooms, optional shrine, boss arena) → victory or death → back to class select. Permadeath.

Language: **Rust**, edition 2024, single lib + bin crate. Author is a Python/TS developer new to
Rust — prefer simple, explicit Rust (plain structs, enums + `match`, `slotmap` ids, no ECS).

---

## 2. Timeline

All work so far happened on **2026-10-08** (planning session + implementation the same day).

| When (2026-10-08) | Commit | What |
|---|---|---|
| morning | — | Design interview ("grilling"); plan in `~/.claude/plans/i-want-to-create-groovy-nautilus.md` |
| 10:50 | `af2353f` | **M0** platform skeleton: `CellGrid`, `Backend` trait, terminal / window / headless backends, diffed terminal writer, fixed-timestep loop, F1 overlay, sandbox screen |
| 11:03 | `c1d0f3d` | **M1** world: iso maths, stamp + room parsers, walls/floors/props, lighting, FOV, fog of war, front-wall stumps, `--png` screenshots |
| 11:11 | `6bd6820` | **M2** 3D figures: RON figure format + animation, analytic raycaster (2×4 samples/cell), ASCII shader, x-ray outlines, Gallery screen |
| 11:50 | `171db18` | **M3–M7** combat, 4 classes, 5 enemies + boss, abilities/statuses/resources, AI, pathfinding, level generator, room flow + waves, upgrades, HUD, menus (main, class select, pause, settings, controls), autoplay bot, integration tests |
| 11:53 | `676217c` | README, bigger dev-arena waves, headless bot exits when the run ends |
| 11:55 | `19932d6` | Clippy clean-up, audio code fixes (volume applied once, loop region) |
| 11:56 | `09b3ce4` | Art fixes: brazier stamp, golem glyph ramps, shrine start marker, darker reward backdrop |
| 11:57 | `53953cb` | Screenshots in `docs/`, linked from README |
| afternoon | (this commit) | `SPEC.md` (this file) and `CLAUDE.md` pointing new sessions to it |

Size at that point: ~10.7k lines of Rust in `src/`, 18 figures, 12 room templates, 3 stamp files.

---

## 3. Settled design decisions

Made with the user in the design interview. Don't reopen these without asking.

| Topic | Decision |
|---|---|
| Pacing | Real-time, Diablo-style: LMB move/attack, RMB special, Q skill, Space mobility; WASD when the backend reports key releases, otherwise mouse-move mode |
| Platforms | Terminal **and** native window, both in the demo; game code only writes a `CellGrid` |
| Render split | Hybrid: hand-drawn stamps for the dungeon, 3D primitive figures + ASCII shader for actors |
| Look | Printable ASCII in the world, box-drawing in UI; 24-bit colour on black, 256-colour fallback |
| Heroes | Pick **one** hero per run: Knight, Wizard, Rogue, Archer; 4 actions each |
| Resources | Per-class resource (Fury / Mana / Energy / Focus) — the user chose this over cooldowns-only; mobility skills also have cooldowns |
| Models | Composite primitive figures defined in RON, animated by moving parts |
| Enemies | Skeleton, Skeleton Archer, Slime (splits), Bat; boss Bone Golem (slam, summons at 50%, enrages < 25%) |
| Level | Seeded generator chaining hand-drawn room templates |
| Room flow | Dark exploration + fog of war; combat rooms seal, enemies come in waves, clearing opens doors |
| Rewards | Pick 1 of 3 upgrades on room clear; health / resource orbs drop |
| Camera | Fixed 2:1 dimetric, **one art scale**; zoom = font size (terminal font zoom / window mouse wheel) |
| Occlusion | Front walls drawn as short stumps + coloured x-ray outline for hidden actors |
| Audio | Synthesised SFX + ambient loop via kira, opt-in `audio` feature |
| Authoring | Plain text / RON files + in-game Gallery + F5 hot reload |
| Run | 10–15 min target, permadeath, one difficulty |
| Reference terminals | Ghostty (installed on the dev machine) and kitty |

Dev machine: Pop!_OS 24.04, COSMIC on Wayland, Ghostty, Rust 1.93. ALSA dev headers were **not**
installed on 2026-10-08.

---

## 4. Build & run

```bash
cargo run --release                         # terminal
cargo run --release -- --window             # native window (X11 via XWayland)
cargo run --release --features audio        # with sound (needs libasound2-dev + pkg-config)
cargo test                                  # all tests
cargo run --release -- --headless --bot --god --class archer --frames 20000   # soak test
cargo run --release -- --headless --screen gallery --focus knight --png out.png  # screenshot
```

Cargo features: `terminal`, `window` (default on); `audio`, `window-wayland` (opt-in).
`cargo check --no-default-features` builds only the simulation, renderer and headless backend.

Useful dev flags: `--class`, `--room NAME` (single room, e.g. `test`, `test_boss`, `shrine`),
`--seed`, `--god`, `--bot`, `--screen menu|classes|game|gallery|sandbox`, `--focus`,
`--headless --frames N --size WxH --dump --png FILE`, `--allow-small`, `--mouse-only`.
In-game dev keys: F1 overlay, F2 reveal map, F3 god mode, F4 kill all enemies, F5 reload assets.

Visual iteration loop used so far: render with `--headless --png`, then crop/zoom with Python PIL
and inspect the image.

---

## 5. Architecture

```
src/game/      pure simulation: no I/O, no clocks, no HashMap (enforced by tests/architecture.rs)
src/render/    iso maths, stamps + lighting, figures, raycaster, ASCII shader, fx, UI widgets
src/assets/    loads + validates everything under assets/ (errors carry file and line)
src/platform/  Backend trait; terminal, window, headless backends; glyph rasteriser; audio
src/app/       main loop, screen stack, settings, debug overlay, bot
```

### Contracts
- `render::cell::{Cell, CellGrid, Rgb}` — the only thing backends receive.
- `platform::Backend` — `caps`, `size`, `poll(&mut InputState)`, `present(&CellGrid)`,
  `wait_until`, `invalidate`, `set_color_mode`.
- `platform::InputState` — held keys + pressed/released edges latched until the first sim tick
  of a frame consumes them; mouse in fractional cell coords.
- `game::world::TickInput` — what the sim sees: move dir, move-to point, aim point, hovered
  target, 4 action pressed/held/released, reward pick.
- `app::Screen` — `update` (one 60 Hz tick), `render`, `overlay`, `resumed`; transitions are
  `Push / Pop / Replace / Reset / Quit`.

### Main loop (`src/app/mod.rs`)
Poll input → global keys (F1, F5) → up to 5 fixed ticks of 1/60 s → render the screen stack
(from the top-most non-overlay screen up) → toasts → debug overlay → `present` → adaptive frame
cap (60 → 45 → 30 fps if writes are slow or > 150 KB). Below 120×36 a "terminal too small"
screen replaces the game.

### Simulation tick order (`World::step`)
Reward / dead / victory states short-circuit. Otherwise: hero control → enemy AI → casts →
movement (walk, dash, leap, knockback, tile collision, actor separation) → projectiles → areas →
statuses + resources + cooldowns → deaths (drops, slime split) → pickups → room flow → FOV →
flow field rebuild → clean-up. Same seed + same inputs = same world hash (tested).

### Rendering pipeline (game screen)
1. Camera follows the hero with a dead zone; integer cells; screen-shake offset.
2. Lights: hero light (masked by hero FOV), torches/braziers (each with its own FOV mask),
   projectile and burning-ground lights.
3. `render::world::draw_world` — stamps per tile, painter's order + **height test**, lit per cell,
   glyphs dimmed in the dark (`glyph_dim.ron`), remembered tiles drawn as a faint blue map.
   Fills `WorldLayer` (per-cell height, face, walkable-floor flag).
4. Ground decals: area telegraphs (fill grows as the wind-up progresses), enemy wind-up cones /
   lines / circles, move marker, shadows under flyers and projectiles, burning ground.
5. Figures → `render::scene::Scene::draw`: pose (`figure::pose`) → raycast
   (`raycast::raycast_figure`, 2×4 samples per cell) → `ascii_shader::resolve`.
6. Mouse picking from the sample buffer's owner ids (fallback: nearest enemy to the ground point).
7. `fx::Fx` particles, damage numbers, slash trails; enemy HP bars; stun stars; cursor.
8. HUD, boss bar, then overlays (reward cards, death / victory).

### Key maths (`src/render/iso.rs`)
`col = 8(x − z)`, `row = 2(x + z) − 2√6·y`; floor tile = 16×4 cell diamond; hero ≈ 9 rows.
At a fixed screen position a **higher** surface is **nearer**, so depth testing is a height
comparison — stamp cells carry heights from their proxy box, so figures hide behind walls.

### ASCII shader (`src/render/ascii_shader.rs`, tuned by `assets/shader.ron`)
Occluded samples (below the dungeon surface) → x-ray outline in faction colour. Visible cells:
outline glyph (`' - . _ / \ |`, chosen from edge orientation via a structure tensor) when the
silhouette passes *through* the cell or depth jumps; otherwise a fill glyph from the material's
ramp by lighting (fixed key light + ambient + rim + dungeon lights). Current defaults:
`edge_depth 0.6`, `edge_normal -0.2`, `edge_min 3`, `coverage_min 3`, `edge_material false`.

### Terminal output (`src/platform/term_writer.rs`)
Front-buffer diff; only changed cells; colour codes only when they change; short same-colour gaps
re-printed instead of cursor jumps; frames wrapped in `?2026h/l`; nothing sent if nothing changed.
Terminal setup: raw mode, alternate screen, cursor hidden, autowrap off, mouse capture, focus
events, kitty keyboard flags (`DISAMBIGUATE | REPORT_EVENT_TYPES | REPORT_ALL_KEYS_AS_ESCAPE_CODES`)
when `supports_keyboard_enhancement()`; restore on drop and in the panic hook. Without release
events, presses become timed "pulses" (0.55 s first, 0.12 s on repeat).

---

## 6. Gameplay as built

### Heroes (`assets/classes.ron`, `assets/abilities.ron`)
| Class | HP / armour / speed | Resource | LMB | RMB | Q | Space |
|---|---|---|---|---|---|---|
| Knight | 140 / 35 / 3.3 | Fury (start 20, +7 hit, +6 hurt, decays) | Sword Arc (cleave 3) | Shield Block (hold, −80% frontal) | Shield Bash (cone stun, 30 Fury) | Charge (pass-through dash, knockback) |
| Wizard | 75 / 5 / 3.6 | Mana (regen 13/s) | Arcane Bolt | Fireball (explodes) | Frost Nova (freeze) | Blink |
| Rogue | 90 / 15 / 4.3, 15% crit | Energy (regen 22/s) | Dagger Flurry (×3 from behind) | Knife Fan (3 knives) | Smoke Bomb (invisible, next hit crits, enemies lose you) | Shadow Dash (i-frames) |
| Archer | 85 / 10 / 3.9 | Focus (start 40, +8 per hit, decays) | Quick Shot | Piercing Shot (hold to charge, ×2.5, pierces) | Volley (arrow rain) | Evasive Roll (i-frames) |

Effect primitives (`game::defs::Effect`): `MeleeArc`, `Projectile`, `AreaAtAim`, `AreaSelf`,
`Dash`, `Blink`, `Block`, `Buff`, `DropAggro`, `Summon`, `Leap`.
Statuses: Stun, Freeze, Slow, Invisible, IFrames, Burning, Poison, NextHitCrit, Blocking, Enraged,
Haste. Bosses turn stun/freeze into a 40% slow.
Damage: × damage mult, crit ×2, backstab mult, armour `1 − a/(a+50)`, frontal block reduction.
Hero input: click ground = A* walk; click / hold enemy = walk up and attack (ranged heroes also
need a clear line of fire); WASD + click = attack towards cursor; 0.2 s input buffer for skills.

### Enemies (`assets/enemies.ron`)
Skeleton (melee, telegraphed slash), Skeleton Archer (keeps 3.5–6.5 tiles, telegraphed shot),
Slime (hops with landing splash, splits into 2 Slimelets), Bat (flies, orbits, swoops),
Bone Golem boss (650 HP; telegraphed slam, hurls bone at range, summons 4 skeletons below 50%,
enrages below 25%). Enemy HP scales `1 + 0.08 × room depth`. AI uses a shared flow field
towards the hero, or walks straight when the line is clear.

### Dungeon (`src/game/levelgen.rs`, `assets/rooms/*.room`)
Plan: start → 4–6 combat rooms (shrine inserted with 75% chance) → boss arena. Each room is
attached through a free door slot: straight 1-wide corridor of 2–5 tiles, new room placed so an
opposite-facing door meets it; no overlaps (1-tile margin); 80 tries per room, 40 seeded
restarts. Templates: `start_a`, `start_b`, 6 combat rooms, `shrine`, `boss_arena`; dev:
`test`, `test_boss`.

### Room flow (`src/game/room_flow.rs`)
Entering a combat room interior seals its doors, spawns wave 0 (enemies rise out of the floor).
Wave budget `2.5 + 1.1·depth + 1.2·wave`; 2 waves per room (+1 from the 4th combat room).
Clearing opens doors and offers 1 of 3 upgrades. Shrine: heal orb + upgrade. Boss room: victory
when the golem dies (its summons crumble).

### Upgrades (`assets/upgrades.ron`)
7 generic (HP, damage, speed, cooldowns, crit, armour, resource max) + 4 per class. Stat or
ability-field modifiers (`(value + add) × (1 + mul)`), extra on-hit statuses, heals. Hero stats
and abilities are rebuilt from class + all upgrades each time one is taken.

### Screens (`src/app/screens/`)
Main menu (title + four turning heroes), class select (cards with 1.5× turning hero, stats,
skills), game, pause, settings (volume, FPS, movement mode, colours, shake, frame cap — saved to
`~/.config/glyphcrawl/settings.ron`), controls, gallery (every figure and stamp; Tab cycles
animations), sandbox (M0 input test).

---

## 7. Verification so far (2026-10-08)

- `cargo test`: 38 tests — level generation over 300 seeds (start first, boss last, 4–6 combat
  rooms, all rooms reachable, no overlaps, doors on edges, ≥2 spawns), same-seed determinism for
  all 4 classes over 3,600 ticks, FOV symmetry, A* / flow field, collisions, combat maths, RNG,
  room/stamp parsers, iso round-trips, terminal diff encoder, the "game/ is pure" check, and
  insta snapshots of each primitive at 4 angles plus the x-ray case.
- `cargo clippy --all-targets`: clean.
- Bot, god mode: 50 seeds (100–149, classes rotating) — all reached victory, no panics.
- Bot, no god mode: typically clears ~5 rooms and dies around the end (~2 min of game time).
- Render cost ~0.4–0.6 ms/frame (release); ~4–8 KB/frame to the terminal when the camera is
  still, ~45 KB for a full redraw.
- Terminal backend ran correctly under a pseudo-terminal (`script` + `strace`).

**Not verified yet:**
- Real play by a person in Ghostty/kitty (feel, controls, balance).
- `--window` backend on screen (never opened — no virtual X server on the dev machine).
- `audio` feature: written against kira 0.12 docs but **never compiled** (ALSA headers missing).
- Human run length vs. the 10–15 min target (bot runs take 1.5–4 min).

---

## 8. Known gaps and ideas

- Install `libasound2-dev pkg-config`, then `cargo build --features audio` and fix any errors.
- Balance pass with real play (wave sizes, boss HP, class damage); all values live in RON.
- Corridors are straight and 1 tile wide (plan had L-shaped, 2-wide).
- Game-scale figures are small (~9 rows); readability depends on `shader.ron` tuning.
- Possible next steps: party / co-op heroes, Diablo-style loot, more levels, gamepad in the
  window backend, shape-matching ASCII shader.

---

## 9. Keeping this spec current

**This file must be updated with every new feature or change.** When you add, change or remove
a feature, system, asset format, design decision, CLI flag or test — or verify something listed
as unverified — update the relevant section here **and** add a row to the timeline in §2 (date,
commit, what changed) in the same commit. A spec that disagrees with the code is worse than none.
