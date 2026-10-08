//! Pick a hero. Each card shows the hero turning slowly, their stats and their four skills.

use crate::app::{Ctx, Screen, Transition};
use crate::platform::audio::Sfx;
use crate::platform::{InputState, Key, MouseButton};
use crate::render::cell::{CellGrid, Rgb};
use crate::render::scene::Scene;
use crate::render::ui;
use crate::render::world::WorldLayer;

pub struct ClassSelect {
    sel: usize,
    t: f32,
    ids: Vec<String>,
    cards: Vec<(i32, i32, i32, i32)>,
    scene: Scene,
    layer: WorldLayer,
}

impl ClassSelect {
    pub fn new(ctx: &Ctx) -> ClassSelect {
        // Fixed order matching the design: Knight, Wizard, Rogue, Archer (then anything else).
        let mut ids: Vec<String> = ["knight", "wizard", "rogue", "archer"]
            .iter()
            .filter(|k| ctx.assets.defs.classes.contains_key(**k))
            .map(|s| s.to_string())
            .collect();
        for k in ctx.assets.defs.classes.keys() {
            if !ids.contains(k) {
                ids.push(k.clone());
            }
        }
        ClassSelect { sel: 0, t: 0.0, ids, cards: Vec::new(), scene: Scene::default(), layer: WorldLayer::default() }
    }

    fn start(&self, ctx: &mut Ctx) -> Transition {
        ctx.sfx(Sfx::UiSelect);
        let seed = ctx.opts.seed.unwrap_or_else(super::fresh_seed);
        let class = &self.ids[self.sel];
        match super::game::GameScreen::new_run(ctx, class, seed, ctx.opts.room.clone().as_deref()) {
            Ok(g) => Transition::Replace(Box::new(g)),
            Err(e) => {
                ctx.toast(format!("could not start: {e:#}"), Rgb(255, 90, 90));
                Transition::None
            }
        }
    }
}

impl Screen for ClassSelect {
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition {
        self.t += dt;
        let n = self.ids.len().max(1);
        if input.was_pressed(Key::Right) || input.was_pressed(Key::Char('d')) {
            self.sel = (self.sel + 1) % n;
            ctx.sfx(Sfx::UiMove);
        }
        if input.was_pressed(Key::Left) || input.was_pressed(Key::Char('a')) {
            self.sel = (self.sel + n - 1) % n;
            ctx.sfx(Sfx::UiMove);
        }
        for (i, key) in ['1', '2', '3', '4'].iter().enumerate() {
            if i < n && input.was_pressed(Key::Char(*key)) {
                self.sel = i;
                return self.start(ctx);
            }
        }
        let mouse = input.mouse_cell();
        for (i, &(x, y, w, h)) in self.cards.iter().enumerate() {
            if ui::hover(mouse, x, y, w, h) {
                self.sel = i;
                if input.clicked(MouseButton::Left) {
                    return self.start(ctx);
                }
            }
        }
        if input.was_pressed(Key::Enter) || input.was_pressed(Key::Space) {
            return self.start(ctx);
        }
        if input.back() {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, _input: &InputState) {
        let (w, h) = (grid.w as i32, grid.h as i32);
        grid.text_centered(1, "═══  CHOOSE YOUR HERO  ═══", ui::UI_GOLD);
        grid.text_centered(2, "click a hero · ←/→ and Enter · 1-4 · Esc back", ui::UI_DIM);
        let n = self.ids.len() as i32;
        let gap = 2;
        let card_w = ((w - 4 - gap * (n - 1)) / n.max(1)).clamp(24, 40);
        let total = n * card_w + (n - 1) * gap;
        let x0 = (w - total) / 2;
        let y0 = 4;
        let card_h = h - y0 - 2;
        self.cards.clear();
        let defs = &ctx.assets.defs;
        for (i, id) in self.ids.clone().iter().enumerate() {
            let Some(c) = defs.classes.get(id) else { continue };
            let x = x0 + i as i32 * (card_w + gap);
            let selected = i == self.sel;
            let col = Rgb::from_hex(&c.color).unwrap_or(ui::UI_GOLD);
            // Black card background: the floor and figure are drawn on black.
            if selected {
                ui::panel_double(grid, x, y0, card_w, card_h, col, Rgb::BLACK);
            } else {
                ui::panel(grid, x, y0, card_w, card_h, ui::UI_FRAME, Rgb::BLACK, None);
            }
            grid.text(x + 2, y0 + 1, &format!("[{}] {}", i + 1, c.name.to_uppercase()), if selected { col } else { ui::UI_TEXT });
            // The hero, turning (faster when selected), playing a skill animation now and then.
            let angle = 0.7 + self.t * if selected { 0.9 } else { 0.25 };
            let clip = if selected && (self.t % 3.0) > 2.2 {
                defs.abilities.get(&c.abilities[0]).map(|a| a.anim.clone()).unwrap_or_default()
            } else {
                "idle".into()
            };
            let ct = if clip == "idle" { self.t } else { (self.t % 3.0) - 2.2 };
            let fig_y = y0 + 18.min(card_h / 2);
            super::draw_figure_at(ctx, &mut self.scene, &mut self.layer, grid, &c.figure, x + card_w / 2, fig_y, angle, 1.5, &clip, ct, true);
            let mut y = fig_y + 5;
            for line in ui::wrap(&c.desc, (card_w - 4) as usize) {
                grid.text(x + 2, y, &line, ui::UI_DIM);
                y += 1;
            }
            y += 1;
            let stat = |grid: &mut CellGrid, y: i32, label: &str, v: f32, max: f32, col: Rgb| {
                grid.text(x + 2, y, label, ui::UI_TEXT);
                ui::bar(grid, x + 10, y, (card_w - 12).min(16), v / max, col, Rgb(50, 50, 55));
            };
            stat(grid, y, "health", c.hp, 150.0, Rgb(220, 70, 70));
            stat(grid, y + 1, "armour", c.armor, 40.0, Rgb(160, 170, 190));
            stat(grid, y + 2, "speed", c.speed, 4.5, Rgb(120, 220, 140));
            grid.text(x + 2, y + 3, &format!("resource: {}", c.resource.kind.label()), Rgb(150, 190, 255));
            y += 5;
            let keys = ["LMB", "RMB", " Q ", "SPC"];
            for (k, aid) in c.abilities.iter().enumerate() {
                if y >= y0 + card_h - 1 {
                    break;
                }
                let name = defs.abilities.get(aid).map(|a| a.name.as_str()).unwrap_or("?");
                grid.text(x + 2, y, keys[k], ui::UI_GOLD);
                let maxw = (card_w - 9).max(4) as usize;
                let name: String = name.chars().take(maxw).collect();
                grid.text(x + 6, y, &name, ui::UI_TEXT);
                y += 1;
            }
            self.cards.push((x, y0, card_w, card_h));
        }
    }
}
