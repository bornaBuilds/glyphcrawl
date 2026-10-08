//! The game itself: owns the `World`, turns input into `TickInput`, draws everything.

use glam::{Vec2, Vec3};

use crate::app::{Ctx, Screen, Transition};
use crate::game::actor::{ActorId, CastPhase, Faction};
use crate::game::defs::{Effect, StatusKind};
use crate::game::level::{Level, RoomKind, RoomState};
use crate::game::world::{GameEvent, PickupKind, RunState, TickInput, World, HERO_LIGHT_RADIUS, TORCH_RADIUS};
use crate::platform::audio::Sfx;
use crate::platform::{InputState, Key, MouseButton};
use crate::render::ascii_shader::FigureStyle;
use crate::render::cell::{CellGrid, Rgb};
use crate::render::figure::ClipPlay;
use crate::render::fx::{draw_decal, draw_flames, Decal, Fx};
use crate::render::iso::{screen_dir_to_world, Camera};
use crate::render::raycast::{SX, SY};
use crate::render::scene::{FigureInst, Scene};
use crate::render::ui;
use crate::render::world::{draw_world, flicker, LightSrc, WorldLayer, WorldView};

pub struct GameScreen {
    pub world: World,
    pub class: String,
    cam: Camera,
    layer: WorldLayer,
    scene: Scene,
    fx: Fx,
    cam_init: bool,
    reveal: bool,
    /// Enemy under the mouse (from the last rendered frame).
    hover: Option<ActorId>,
    /// Which actor each drawn figure belongs to.
    fig_owner: Vec<Option<ActorId>>,
    reward_cards: Vec<(i32, i32, i32, i32)>,
    reward_sel: usize,
    move_marker: Option<(Vec2, f32)>,
}

impl GameScreen {
    pub fn new(world: World, class: &str) -> GameScreen {
        GameScreen {
            world,
            class: class.to_string(),
            cam: Camera::default(),
            layer: WorldLayer::default(),
            scene: Scene::default(),
            fx: Fx::default(),
            cam_init: false,
            reveal: false,
            hover: None,
            fig_owner: Vec::new(),
            reward_cards: Vec::new(),
            reward_sel: 0,
            move_marker: None,
        }
    }

    /// Start a run: a generated dungeon, or a single room template with `room`.
    pub fn new_run(ctx: &Ctx, class: &str, seed: u64, room: Option<&str>) -> anyhow::Result<GameScreen> {
        let level = match room {
            Some(name) => {
                let idx = ctx.assets.room(name).ok_or_else(|| anyhow::anyhow!("no room template named '{name}'"))?;
                Level::single_room(&ctx.assets.rooms, idx)
            }
            None => crate::game::levelgen::generate(&ctx.assets.rooms, seed).map_err(|e| anyhow::anyhow!(e))?,
        };
        let mut world = World::new(level, &ctx.assets.defs, class, seed);
        world.god = ctx.opts.god;
        Ok(GameScreen::new(world, class))
    }

    fn mouse_ground(&self, input: &InputState) -> Option<Vec2> {
        input.mouse.map(|(x, y)| self.cam.grid_to_plane(Vec2::new(x, y), 0.0))
    }

    fn tick_input(&mut self, ctx: &Ctx, input: &InputState) -> TickInput {
        let mut ti = TickInput::default();
        if ctx.wasd() {
            let mut right = 0.0;
            let mut down = 0.0;
            if input.is_held(Key::Char('w')) || input.is_held(Key::Up) {
                down -= 1.0;
            }
            if input.is_held(Key::Char('s')) || input.is_held(Key::Down) {
                down += 1.0;
            }
            if input.is_held(Key::Char('a')) || input.is_held(Key::Left) {
                right -= 1.0;
            }
            if input.is_held(Key::Char('d')) || input.is_held(Key::Right) {
                right += 1.0;
            }
            ti.move_dir = screen_dir_to_world(right, down);
        }
        let hero_pos = self.world.hero().pos;
        ti.aim = self.mouse_ground(input).unwrap_or(hero_pos + self.world.hero().facing_dir());
        ti.target = self.hover;
        ti.lmb_ground = self.hover.is_none() && input.mouse.is_some();
        let l = MouseButton::Left as usize;
        let r = MouseButton::Right as usize;
        ti.act_pressed[0] = input.btn_pressed[l] || input.was_pressed(Key::Char('1'));
        ti.act_held[0] = input.btn_down[l] || input.is_held(Key::Char('1'));
        ti.act_released[0] = input.btn_released[l];
        // Right-click, or E / 2 for trackpads.
        ti.act_pressed[1] = input.btn_pressed[r] || input.was_pressed(Key::Char('e')) || input.was_pressed(Key::Char('2'));
        ti.act_held[1] = input.btn_down[r] || input.is_held(Key::Char('e')) || input.is_held(Key::Char('2'));
        ti.act_released[1] = input.btn_released[r];
        ti.act_pressed[2] = input.was_pressed(Key::Char('q')) || input.was_pressed(Key::Char('3'));
        ti.act_held[2] = input.is_held(Key::Char('q')) || input.is_held(Key::Char('3'));
        ti.act_pressed[3] = input.was_pressed(Key::Space) || input.was_pressed(Key::Char('4'));
        ti.act_held[3] = input.is_held(Key::Space) || input.is_held(Key::Char('4'));
        if input.btn_pressed[l] && self.hover.is_none() && ti.move_dir == Vec2::ZERO {
            self.move_marker = Some((ti.aim, 0.6));
        }
        // Reward picking.
        if let RunState::Reward { choices } = &self.world.state {
            let n = choices.len();
            ti = TickInput { aim: ti.aim, ..Default::default() };
            for (k, key) in ['1', '2', '3'].iter().enumerate() {
                if k < n && input.was_pressed(Key::Char(*key)) {
                    ti.pick = Some(k);
                }
            }
            if input.was_pressed(Key::Left) || input.was_pressed(Key::Char('a')) {
                self.reward_sel = (self.reward_sel + n - 1) % n.max(1);
            }
            if input.was_pressed(Key::Right) || input.was_pressed(Key::Char('d')) {
                self.reward_sel = (self.reward_sel + 1) % n.max(1);
            }
            if input.was_pressed(Key::Enter) {
                ti.pick = Some(self.reward_sel.min(n.saturating_sub(1)));
            }
            let mouse = input.mouse_cell();
            for (k, &(x, y, w, h)) in self.reward_cards.iter().enumerate() {
                if ui::hover(mouse, x, y, w, h) {
                    self.reward_sel = k;
                    if input.clicked(MouseButton::Left) {
                        ti.pick = Some(k);
                    }
                }
            }
        }
        ti
    }

    fn handle_events(&mut self, ctx: &mut Ctx) {
        let events = std::mem::take(&mut self.world.events);
        for e in &events {
            self.fx.on_event(e);
            match e {
                GameEvent::Sfx(name) => {
                    if let Some(s) = Sfx::from_name(name) {
                        ctx.sfx(s);
                    }
                }
                GameEvent::Message(m) => ctx.toast(m.clone(), ui::UI_GOLD),
                GameEvent::DoorsSealed => ctx.toast("The doors slam shut!", Rgb(255, 120, 90)),
                _ => {}
            }
        }
        if !ctx.settings.screen_shake {
            self.fx.shake = 0.0;
        }
    }

    /// Restart with the same class and a new seed.
    fn restart(&self, ctx: &Ctx) -> Transition {
        let seed = self.world.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 16;
        match GameScreen::new_run(ctx, &self.class, seed, ctx.opts.room.as_deref()) {
            Ok(g) => Transition::Replace(Box::new(g)),
            Err(_) => Transition::Pop,
        }
    }
}

impl Screen for GameScreen {
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        if ctx.opts.headless && ctx.opts.bot {
            if let RunState::Dead { t } | RunState::Victory { t } = self.world.state {
                if t > 1.0 {
                    return Transition::Quit;
                }
            }
        }
        match self.world.state {
            RunState::Dead { t } | RunState::Victory { t } if t > 1.2 => {
                if input.was_pressed(Key::Enter) || input.clicked(MouseButton::Left) {
                    if ctx.opts.class.is_some() || ctx.opts.room.is_some() {
                        return self.restart(ctx);
                    }
                    return Transition::Replace(Box::new(super::class_select::ClassSelect::new(ctx)));
                }
                if input.was_pressed(Key::Esc) {
                    return Transition::Reset(Box::new(super::main_menu::MainMenu::new()));
                }
            }
            RunState::Playing => {
                if input.was_pressed(Key::Esc) {
                    return Transition::Push(Box::new(super::pause::Pause::new()));
                }
            }
            _ => {}
        }
        // Dev keys.
        if input.was_pressed(Key::F(2)) {
            self.reveal = !self.reveal;
        }
        if input.was_pressed(Key::F(3)) {
            self.world.god = !self.world.god;
            ctx.toast(if self.world.god { "god mode on" } else { "god mode off" }, Rgb(200, 200, 255));
        }
        if input.was_pressed(Key::F(4)) {
            for id in self.world.enemy_ids() {
                let a = &mut self.world.actors[id];
                a.hp = 0.0;
                a.dead = true;
                a.start_action("die");
            }
        }
        ctx.run_summary = format!(
            "{} | {:?} | rooms cleared {} | kills {} | hp {:.0}/{:.0} | time {:.0}s | upgrades {}",
            self.class,
            match self.world.state {
                RunState::Playing => "playing",
                RunState::Reward { .. } => "reward",
                RunState::Dead { .. } => "DEAD",
                RunState::Victory { .. } => "VICTORY",
            },
            self.world.stats.rooms_cleared,
            self.world.stats.kills,
            self.world.hero().hp,
            self.world.hero().stats.max_hp,
            self.world.stats.time,
            self.world.hero().upgrades.iter().map(|u| u.1).sum::<u32>()
        );
        let ti = if ctx.opts.bot { crate::app::bot::think(&self.world, &ctx.assets.defs) } else { self.tick_input(ctx, input) };
        self.world.step(&ctx.assets.defs, &ti, dt);
        self.handle_events(ctx);
        self.fx.update(dt);
        if let Some((_, t)) = &mut self.move_marker {
            *t -= dt;
        }
        if self.move_marker.is_some_and(|(_, t)| t <= 0.0) {
            self.move_marker = None;
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, input: &InputState) {
        let hero = self.world.hero().clone();
        let hero_p = Vec3::new(hero.pos.x, 0.0, hero.pos.y);
        // Keep the hero a bit above centre: the HUD covers the bottom rows.
        let view_h = grid.h.saturating_sub(6).max(10);
        if !self.cam_init {
            self.cam.center_on(hero_p, grid.w, view_h);
            self.cam_init = true;
        } else {
            self.cam.follow(hero_p, grid.w, view_h, (grid.w / 10) as i32, (grid.h / 9) as i32);
        }
        let (sx, sy) = self.fx.shake_offset();
        let cam = Camera { col: self.cam.col + sx, row: self.cam.row + sy };
        let time = self.world.time;
        let w = &self.world;
        let map = w.map();

        // ---- Lights.
        let mut lights: Vec<LightSrc> = Vec::with_capacity(w.torches.len() + 8);
        lights.push(LightSrc {
            pos: hero_p + Vec3::new(0.0, 1.6, 0.0),
            color: [1.5, 1.25, 0.95],
            radius: HERO_LIGHT_RADIUS + 1.5,
            mask: Some(&w.hero_fov),
        });
        for t in &w.torches {
            let f = 0.85 + 0.25 * flicker(time, t.tile.x, t.tile.y);
            lights.push(LightSrc { pos: t.pos, color: [1.7 * f, 0.95 * f, 0.45 * f], radius: TORCH_RADIUS + 1.5, mask: Some(&t.mask) });
        }
        for p in &w.projectiles {
            let c = match p.figure.as_str() {
                "proj_fireball" => Some(([1.8, 0.8, 0.3], 4.0)),
                "proj_bolt" => Some(([0.9, 0.6, 1.5], 2.5)),
                "proj_arrow_big" => Some(([1.3, 1.1, 0.4], 2.5)),
                _ => None,
            };
            if let Some((color, radius)) = c {
                lights.push(LightSrc { pos: Vec3::new(p.pos.x, p.y + 0.3, p.pos.y), color, radius, mask: None });
            }
        }
        for a in &w.areas {
            if a.linger_left > 0.0 && matches!(a.def.fx, crate::game::defs::AreaFx::Fire) {
                lights.push(LightSrc { pos: Vec3::new(a.pos.x, 0.6, a.pos.y), color: [1.4, 0.6, 0.2], radius: a.def.radius + 2.0, mask: None });
            }
        }

        // ---- Dungeon.
        let all = vec![true; w.visible.len()];
        let view = WorldView {
            map,
            visible: if self.reveal { &all } else { &w.visible },
            seen: if self.reveal { &all } else { &w.seen },
            lights: &lights,
            ambient: [0.06, 0.065, 0.1],
            time,
        };
        draw_world(grid, &mut self.layer, &cam, &view, &ctx.assets.world_art());

        // ---- Ground decals.
        for a in &w.areas {
            if a.delay_left > 0.0 && w.tile_visible(a.pos) {
                let total = a.def.delay.max(0.01);
                let progress = 1.0 - a.delay_left / total;
                let color = if a.faction == Faction::Enemy { Rgb(255, 70, 50) } else { Rgb(240, 200, 90) };
                draw_decal(grid, &self.layer, &cam, Decal::Circle { c: a.pos, r: a.def.radius }, progress, color, ':');
            } else if a.linger_left > 0.0 {
                draw_flames(grid, &self.layer, &cam, a.pos, a.def.radius, time);
            }
        }
        for (_, a) in &w.actors {
            if a.faction != Faction::Enemy || a.dead || !w.tile_visible(a.pos) {
                continue;
            }
            let Some(c) = &a.cast else { continue };
            if c.phase != CastPhase::Windup {
                continue;
            }
            let def = &a.slots[c.slot].def;
            let progress = (c.t / def.windup.max(0.01)).min(1.0);
            let red = Rgb(255, 70, 50);
            let dir = (c.aim - a.pos).normalize_or_zero();
            for e in &def.effects {
                let decal = match e {
                    Effect::MeleeArc { range, arc, .. } => Some(Decal::Cone { c: a.pos, dir, range: *range + 0.2, arc: *arc }),
                    Effect::Projectile { range, .. } => Some(Decal::Line { from: a.pos, dir, len: range.min(9.0), width: 0.5 }),
                    Effect::AreaAtAim { max_range, area } => {
                        let d = c.aim - a.pos;
                        let p = if d.length() > *max_range { a.pos + d.normalize() * *max_range } else { c.aim };
                        Some(Decal::Circle { c: p, r: area.radius })
                    }
                    Effect::Leap { dist, land: Some(area), .. } => {
                        let d = c.aim - a.pos;
                        let p = if d.length() > *dist { a.pos + d.normalize() * *dist } else { c.aim };
                        Some(Decal::Circle { c: p, r: area.radius })
                    }
                    Effect::Dash { dist, .. } => Some(Decal::Line { from: a.pos, dir, len: *dist, width: 0.6 }),
                    _ => None,
                };
                if let Some(d) = decal {
                    draw_decal(grid, &self.layer, &cam, d, progress, red, '.');
                }
            }
        }
        if let Some((p, t)) = self.move_marker {
            if t > 0.0 {
                draw_decal(grid, &self.layer, &cam, Decal::Circle { c: p, r: 0.35 * (0.4 + t) }, 0.0, Rgb(140, 220, 140), '.');
            }
        }
        // Shadows under things in the air.
        let mut shadows: Vec<Vec2> = Vec::new();
        for (_, a) in &w.actors {
            if !a.dead && (a.flying || a.y > 0.2) && w.tile_visible(a.pos) {
                shadows.push(a.pos);
            }
        }
        for p in &w.projectiles {
            shadows.push(p.pos);
        }
        for s in shadows {
            let g = cam.world_to_grid(Vec3::new(s.x, 0.0, s.y));
            let (x, y) = (g.x as i32, g.y as i32);
            if grid.in_bounds(x, y) && self.layer.floor[grid.idx(x, y)] {
                grid.put(x, y, '.', Rgb(40, 38, 36));
            }
        }

        // ---- Figures.
        let figs_lib = &ctx.assets.figures;
        let mut figs: Vec<FigureInst> = Vec::new();
        self.fig_owner.clear();
        let mut boss: Option<(String, f32)> = None;
        for (id, a) in &w.actors {
            let is_hero = id == w.hero;
            if !is_hero && !w.tile_visible(a.pos) && !self.reveal {
                continue;
            }
            let Some(fig) = figs_lib.get(&a.figure) else { continue };
            if a.boss && !a.dead {
                boss = Some((a.name.clone(), a.hp / a.stats.max_hp));
            }
            let mut y = a.y;
            if a.spawning > 0.0 {
                y -= (a.spawning / 0.8) * 2.0;
            }
            let mut f = FigureInst::new(fig, Vec3::new(a.pos.x, y, a.pos.y), a.facing);
            f.scale = a.scale;
            let s = a.anim.squash;
            f.squash = Vec3::new(1.0 / s.sqrt(), s, 1.0 / s.sqrt());
            if !a.dead {
                f.clips.push(ClipPlay { name: if a.moving { "walk" } else { "idle" }, t: time + a.pos.x * 0.37, w: 1.0 });
            }
            if let Some((clip, t)) = &a.anim.action {
                if fig.clips.contains_key(clip) {
                    f.clips.push(ClipPlay { name: clip, t: *t, w: 1.0 });
                }
            }
            let (ti, tj) = map.tile_at(a.pos);
            let mut style = FigureStyle {
                flash: (a.anim.flash / 0.12).min(1.0),
                tile: if map.in_bounds(ti, tj) { Some(map.idx(ti, tj)) } else { None },
                ..Default::default()
            };
            if is_hero {
                style.xray = Some(Rgb(80, 220, 255));
                if a.has(StatusKind::Invisible) {
                    style.fade = 0.45;
                    style.tint = Some(Rgb(150, 150, 190));
                }
            } else {
                style.xray = Some(Rgb(200, 70, 60));
                if Some(id) == self.hover {
                    style.edge_tint = Some(Rgb(255, 230, 120));
                }
            }
            if a.has(StatusKind::Freeze) {
                style.tint = Some(Rgb(150, 210, 255));
            } else if a.has(StatusKind::Burning) {
                style.tint = Some(Rgb(255, 170, 110));
            } else if a.has(StatusKind::Poison) {
                style.tint = Some(Rgb(170, 255, 140));
            } else if a.has(StatusKind::Enraged) {
                style.tint = Some(Rgb(255, 140, 120));
            }
            if a.dead && a.death_t > 0.7 {
                style.fade = ((a.death_t - 0.7) / 0.5).min(1.0);
            }
            f.style = style;
            figs.push(f);
            self.fig_owner.push(Some(id));
        }
        for p in &w.projectiles {
            let Some(fig) = figs_lib.get(&p.figure) else { continue };
            let mut f = FigureInst::new(fig, Vec3::new(p.pos.x, p.y, p.pos.y), p.vel.y.atan2(p.vel.x));
            f.clips.push(ClipPlay { name: "idle", t: p.age, w: 1.0 });
            figs.push(f);
            self.fig_owner.push(None);
        }
        for pk in &w.pickups {
            if !w.tile_visible(pk.pos) {
                continue;
            }
            let name = match pk.kind {
                PickupKind::Health(_) => "orb_health",
                PickupKind::Essence(_) => "orb_essence",
            };
            let Some(fig) = figs_lib.get(name) else { continue };
            let mut f = FigureInst::new(fig, Vec3::new(pk.pos.x, 0.0, pk.pos.y), 0.0);
            f.clips.push(ClipPlay { name: "idle", t: pk.age, w: 1.0 });
            figs.push(f);
            self.fig_owner.push(None);
        }
        let a = &ctx.assets;
        self.scene.draw(grid, &self.layer, &cam, &figs, &lights, &a.mats, &a.shader);

        // ---- Hover: which enemy's samples are under the mouse cell?
        self.hover = None;
        if let Some((mx, my)) = input.mouse_cell() {
            let sb = &self.scene.samples;
            let mut best: Option<(u16, f32)> = None;
            if mx >= 0 && my >= 0 {
                for dy in 0..SY {
                    for dx in 0..SX {
                        let (sxx, syy) = (mx as usize * SX + dx, my as usize * SY + dy);
                        if sxx >= sb.w || syy >= sb.h {
                            continue;
                        }
                        let i = syy * sb.w + sxx;
                        let o = sb.owner[i];
                        if o != 0 && best.is_none_or(|(_, y)| sb.y[i] > y) {
                            best = Some((o, sb.y[i]));
                        }
                    }
                }
            }
            if let Some((o, _)) = best {
                if let Some(Some(id)) = self.fig_owner.get(o as usize - 1) {
                    if w.actors.get(*id).is_some_and(|a| a.faction == Faction::Enemy && a.targetable()) {
                        self.hover = Some(*id);
                    }
                }
            }
            if self.hover.is_none() {
                // Generous fallback: an enemy standing near the ground point under the cursor.
                let g = cam.grid_to_plane(Vec2::new(mx as f32 + 0.5, my as f32 + 0.5), 0.0);
                self.hover = w
                    .actors
                    .iter()
                    .filter(|(_, a)| a.faction == Faction::Enemy && a.targetable() && w.tile_visible(a.pos))
                    .map(|(id, a)| (id, a.pos.distance(g) - a.radius))
                    .filter(|(_, d)| *d < 0.45)
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(id, _)| id);
            }
        }

        self.fx.draw(grid, &self.layer, &cam);

        // ---- Overheads: enemy HP bars and stun stars.
        for (_, a) in &w.actors {
            if a.dead || !w.tile_visible(a.pos) || a.spawning > 0.0 {
                continue;
            }
            let head = Vec3::new(a.pos.x, a.y + 2.05 * a.scale, a.pos.y);
            let g = cam.world_to_grid(head);
            if a.faction == Faction::Enemy && a.hp < a.stats.max_hp && !a.boss {
                let wbar = 6;
                let n = ((a.hp / a.stats.max_hp) * wbar as f32).ceil() as i32;
                for i in 0..wbar {
                    let (ch, col) = if i < n { ('=', Rgb(220, 60, 50)) } else { ('-', Rgb(80, 40, 40)) };
                    grid.put(g.x as i32 - wbar / 2 + i, g.y as i32 - 1, ch, col);
                }
            }
            if a.has(StatusKind::Stun) {
                let k = (time * 6.0) as i32 % 3;
                grid.put(g.x as i32 - 1 + k, g.y as i32 - 2, '*', Rgb(255, 230, 100));
            }
        }

        // ---- Cursor.
        if let Some((mx, my)) = input.mouse_cell() {
            let (ch, col) = if self.hover.is_some() { ('X', Rgb(255, 210, 90)) } else { ('+', Rgb(200, 200, 200)) };
            grid.put(mx, my, ch, col);
        }

        draw_hud(grid, ctx, &self.world, boss);
        ctx.debug_line = format!(
            "actors {} proj {} areas {} | hero ({:.1},{:.1}) seed {}",
            self.world.actors.len(),
            self.world.projectiles.len(),
            self.world.areas.len(),
            hero.pos.x,
            hero.pos.y,
            self.world.seed
        );

        // ---- Overlays.
        match &self.world.state {
            RunState::Reward { choices } => {
                let choices = choices.clone();
                self.reward_cards = draw_reward(grid, ctx, &choices, self.reward_sel, &self.world);
            }
            RunState::Dead { t } => draw_end(grid, &self.world, false, *t),
            RunState::Victory { t } => draw_end(grid, &self.world, true, *t),
            RunState::Playing => {}
        }
    }
}

fn draw_hud(grid: &mut CellGrid, ctx: &Ctx, w: &World, boss: Option<(String, f32)>) {
    let (gw, gh) = (grid.w as i32, grid.h as i32);
    let hero = w.hero();
    let defs = &ctx.assets.defs;
    let class = hero.class_id().and_then(|c| defs.classes.get(c));
    let ccol = class.and_then(|c| Rgb::from_hex(&c.color)).unwrap_or(ui::UI_GOLD);
    let top = gh - 5;
    ui::panel(grid, 0, top, gw, 5, ui::UI_FRAME, ui::UI_BG, None);
    let name = class.map(|c| c.name.to_uppercase()).unwrap_or_default();
    grid.text_bg(2, top, &format!(" {name} "), ccol, ui::UI_BG);
    // Health.
    let hp_frac = hero.hp / hero.stats.max_hp;
    grid.text(2, top + 1, "HP", Rgb(255, 110, 110));
    let hp_col = if hp_frac < 0.3 { Rgb(255, 60, 60) } else { Rgb(220, 70, 70) };
    ui::bar(grid, 9, top + 1, 24, hp_frac, hp_col, Rgb(70, 30, 30));
    grid.text(34, top + 1, &format!("{:>3.0}/{:<3.0}", hero.hp.max(0.0), hero.stats.max_hp), ui::UI_TEXT);
    // Resource.
    if let Some(r) = &hero.resource {
        let rc = match r.def.kind {
            crate::game::defs::ResourceKind::Fury => Rgb(255, 120, 60),
            crate::game::defs::ResourceKind::Mana => Rgb(90, 150, 255),
            crate::game::defs::ResourceKind::Energy => Rgb(240, 220, 90),
            crate::game::defs::ResourceKind::Focus => Rgb(120, 230, 160),
        };
        grid.text(2, top + 2, r.def.kind.label(), rc);
        ui::bar(grid, 9, top + 2, 24, r.cur / r.max, rc, rc.scale(0.3));
        grid.text(34, top + 2, &format!("{:>3.0}/{:<3.0}", r.cur, r.max), ui::UI_TEXT);
    }
    // Room info.
    let room_txt = match w.current_room.map(|r| &w.level.rooms[r]) {
        Some(r) => match (r.kind, r.state) {
            (RoomKind::Boss, _) => "BOSS".to_string(),
            (_, RoomState::Active { wave }) => format!("Room {}  wave {}/{}", r.depth, wave + 1, r.waves),
            (RoomKind::Start, _) => "Entrance".to_string(),
            (RoomKind::Treasure, _) => "Shrine".to_string(),
            _ => format!("Room {} (cleared)", r.depth),
        },
        None => "Corridor".to_string(),
    };
    let total = w.level.rooms.iter().filter(|r| matches!(r.kind, RoomKind::Combat)).count();
    grid.text(2, top + 3, &format!("{room_txt}  ·  cleared {}/{}  ·  kills {}", w.stats.rooms_cleared, total, w.stats.kills), ui::UI_DIM);
    // Ability slots.
    let labels = ["LMB", "RMB", " Q ", "SPC"];
    let slot_w = 22;
    let x0 = (gw - 4 * slot_w - 1).max(46);
    for (k, s) in hero.slots.iter().enumerate().take(4) {
        let x = x0 + k as i32 * slot_w;
        let afford = hero.resource.as_ref().is_none_or(|r| r.cur + 1e-3 >= s.def.cost);
        let ready = s.cd <= 0.0 && afford;
        let frame = if ready { Rgb(150, 140, 120) } else { Rgb(70, 66, 60) };
        ui::panel(grid, x, top, slot_w - 1, 4, frame, ui::UI_BG, None);
        grid.text_bg(x + 2, top, &format!(" {} ", labels[k]), if ready { ui::UI_GOLD } else { ui::UI_DIM }, ui::UI_BG);
        let name: String = s.def.name.chars().take((slot_w - 4) as usize).collect();
        grid.text(x + 2, top + 1, &name, if ready { ui::UI_TEXT } else { ui::UI_DIM });
        let status = if s.cd > 0.0 {
            let total = s.def.cooldown.max(0.01);
            ui::bar(grid, x + 2, top + 2, 10, 1.0 - s.cd / total, Rgb(140, 140, 160), Rgb(50, 50, 60));
            format!("{:.1}s", s.cd)
        } else if !afford {
            format!("need {:.0}", s.def.cost)
        } else if let Some(c) = hero.cast.as_ref().filter(|c| c.slot == k && c.phase == CastPhase::Holding && s.def.charge > 0.0) {
            ui::bar(grid, x + 2, top + 2, 10, c.charge / s.def.charge, Rgb(255, 220, 90), Rgb(60, 50, 20));
            "charging".to_string()
        } else if s.def.cost > 0.0 {
            format!("cost {:.0}", s.def.cost)
        } else {
            "ready".to_string()
        };
        grid.text(x + 13, top + 2, &status, if ready { Rgb(140, 220, 140) } else { ui::UI_DIM });
    }
    if !ctx.wasd() {
        grid.text(2, top + 4, " mouse mode: click to move · Q / Space / right-click for skills ", Rgb(200, 150, 100));
    }
    // Boss bar.
    if let Some((name, frac)) = boss {
        let bw = (gw / 2).max(40);
        let x = (gw - bw) / 2;
        grid.text_centered(1, &name.to_uppercase(), Rgb(255, 120, 100));
        ui::bar(grid, x, 2, bw, frac, Rgb(220, 60, 50), Rgb(70, 30, 30));
    }
}

fn draw_reward(grid: &mut CellGrid, ctx: &Ctx, choices: &[usize], sel: usize, w: &World) -> Vec<(i32, i32, i32, i32)> {
    grid.dim(0.35);
    let (gw, gh) = (grid.w as i32, grid.h as i32);
    let card_w = 34;
    let card_h = 11;
    let n = choices.len() as i32;
    let total = n * card_w + (n - 1) * 3;
    let x0 = (gw - total) / 2;
    let y0 = (gh - card_h) / 2 - 2;
    grid.text_centered(y0 - 3, "═══  CHOOSE A BLESSING  ═══", ui::UI_GOLD);
    grid.text_centered(y0 - 2, "click a card, press 1/2/3, or ←/→ and Enter", ui::UI_DIM);
    let mut rects = Vec::new();
    let hero = w.hero();
    for (k, &ui_idx) in choices.iter().enumerate() {
        let u = &ctx.assets.defs.upgrades[ui_idx];
        let x = x0 + k as i32 * (card_w + 3);
        let selected = k == sel;
        let frame = if selected { ui::UI_GOLD } else { ui::UI_FRAME };
        if selected {
            ui::panel_double(grid, x, y0, card_w, card_h, frame, Rgb(26, 22, 16));
        } else {
            ui::panel(grid, x, y0, card_w, card_h, frame, ui::UI_BG, None);
        }
        let tag = match &u.class {
            Some(c) => ctx.assets.defs.classes.get(c).map(|c| c.name.clone()).unwrap_or_default(),
            None => "Any hero".to_string(),
        };
        grid.text(x + 2, y0 + 1, &format!("[{}]", k + 1), ui::UI_DIM);
        grid.text(x + 6, y0 + 1, &u.name, if selected { Rgb(255, 230, 150) } else { ui::UI_TEXT });
        grid.text(x + 2, y0 + 2, &tag, Rgb(150, 170, 200));
        for (i, line) in ui::wrap(&u.desc, (card_w - 4) as usize).iter().enumerate().take(5) {
            grid.text(x + 2, y0 + 4 + i as i32, line, ui::UI_TEXT);
        }
        let have = hero.upgrades.iter().find(|(id, _)| *id == u.id).map(|e| e.1).unwrap_or(0);
        if u.max > 1 {
            grid.text(x + 2, y0 + card_h - 2, &format!("stacks {}/{}", have, u.max), ui::UI_DIM);
        }
        rects.push((x, y0, card_w, card_h));
    }
    rects
}

fn draw_end(grid: &mut CellGrid, w: &World, victory: bool, t: f32) {
    let k = t.min(1.0);
    grid.dim(1.0 - 0.6 * k);
    if t < 0.4 {
        return;
    }
    let (gw, gh) = (grid.w as i32, grid.h as i32);
    let pw = 56;
    let ph = 14;
    let x = (gw - pw) / 2;
    let y = (gh - ph) / 2 - 2;
    let col = if victory { ui::UI_GOLD } else { Rgb(200, 60, 50) };
    ui::panel_double(grid, x, y, pw, ph, col, Rgb(12, 10, 10));
    let title = if victory { "V I C T O R Y" } else { "Y O U   D I E D" };
    grid.text_centered(y + 2, title, col);
    let sub = if victory { "The Bone Golem crumbles. The crypt is quiet." } else { "The dark takes another hero." };
    grid.text_centered(y + 3, sub, ui::UI_DIM);
    let s = &w.stats;
    let mins = (s.time / 60.0) as u32;
    let secs = (s.time % 60.0) as u32;
    let lines = [
        format!("time            {mins}:{secs:02}"),
        format!("rooms cleared   {}", s.rooms_cleared),
        format!("monsters slain  {}", s.kills),
        format!("damage dealt    {:.0}", s.damage_dealt),
        format!("upgrades        {}", w.hero().upgrades.iter().map(|u| u.1).sum::<u32>()),
        format!("seed            {}", w.seed),
    ];
    for (i, l) in lines.iter().enumerate() {
        grid.text(x + 14, y + 5 + i as i32, l, ui::UI_TEXT);
    }
    if t > 1.2 {
        grid.text_centered(y + ph - 2, "Enter — new run   ·   Esc — main menu", ui::UI_GOLD);
    }
}
