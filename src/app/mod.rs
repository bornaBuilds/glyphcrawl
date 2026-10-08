//! The main loop and the screen stack.

pub mod bot;
pub mod debug;
pub mod screens;
pub mod settings;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::assets::Assets;
use crate::platform::audio::{AudioOut, Sfx};
use crate::platform::{Backend, Caps, ColorMode, InputState, Key};
use crate::render::cell::{CellGrid, Rgb};
use settings::{ColorPref, FrameCap, Settings};

pub const MIN_W: u16 = 120;
pub const MIN_H: u16 = 36;
pub const TICK: f32 = 1.0 / 60.0;

/// Options from the command line that screens care about.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub seed: Option<u64>,
    pub class: Option<String>,
    pub room: Option<String>,
    pub god: bool,
    pub bot: bool,
    pub headless: bool,
    pub allow_small: bool,
    pub focus: Option<String>,
}

pub struct Toast {
    pub text: String,
    pub ttl: f32,
    pub color: Rgb,
}

pub struct Ctx {
    pub assets: Assets,
    pub assets_dir: PathBuf,
    pub settings: Settings,
    pub caps: Caps,
    pub audio: Box<dyn AudioOut>,
    pub opts: Options,
    /// Seconds since start, advanced by fixed ticks (deterministic in headless mode).
    pub time: f64,
    pub toasts: Vec<Toast>,
    pub debug: bool,
    /// Extra line for the F1 overlay, filled by the active screen.
    pub debug_line: String,
    pub stats: debug::FrameStats,
    pub quit: bool,
    /// Set by screens that changed settings affecting the backend.
    pub settings_dirty: bool,
    /// One-line run summary (printed at the end of headless runs).
    pub run_summary: String,
}

impl Ctx {
    pub fn toast(&mut self, text: impl Into<String>, color: Rgb) {
        self.toasts.push(Toast { text: text.into(), ttl: 3.0, color });
    }

    pub fn sfx(&mut self, s: Sfx) {
        self.audio.play(s, 1.0);
    }

    pub fn sfx_vol(&mut self, s: Sfx, v: f32) {
        self.audio.play(s, v);
    }

    /// WASD works (backend reports key releases and the player didn't force mouse mode).
    pub fn wasd(&self) -> bool {
        self.caps.key_release && self.settings.input_mode == settings::InputMode::Auto
    }

    pub fn reload_assets(&mut self) {
        match Assets::load(&self.assets_dir) {
            Ok(a) => {
                self.assets = a;
                self.toast("assets reloaded", Rgb(120, 220, 120));
            }
            Err(e) => {
                let msg = format!("reload failed: {e:#}");
                self.toast(msg, Rgb(255, 90, 90));
            }
        }
    }
}

pub enum Transition {
    None,
    Push(Box<dyn Screen>),
    Pop,
    Replace(Box<dyn Screen>),
    /// Clear the whole stack and start fresh with this screen.
    Reset(Box<dyn Screen>),
    Quit,
}

pub trait Screen {
    /// One fixed simulation tick. Input edges are only present on the first tick of a frame.
    fn update(&mut self, ctx: &mut Ctx, input: &InputState, dt: f32) -> Transition;
    fn render(&mut self, ctx: &mut Ctx, grid: &mut CellGrid, input: &InputState);
    /// Overlays (pause, reward pick) draw on top of the screen below them.
    fn overlay(&self) -> bool {
        false
    }
    /// Called when the screen becomes the top of the stack again.
    fn resumed(&mut self, _ctx: &mut Ctx) {}
}

pub struct RunReport {
    pub frames: u64,
    pub render_ms: Vec<f32>,
    pub bytes: Vec<usize>,
    pub last_frame: Option<CellGrid>,
    pub summary: String,
}

fn effective_color(pref: ColorPref, detected: ColorMode) -> ColorMode {
    match pref {
        ColorPref::Auto => detected,
        ColorPref::TrueColor => ColorMode::TrueColor,
        ColorPref::Ansi256 => ColorMode::Ansi256,
    }
}

/// Adaptive frame cap: steps 60 -> 45 -> 30 fps when the terminal can't keep up.
struct Limiter {
    fps: f32,
    good_for: f32,
}

impl Limiter {
    fn adapt(&mut self, cap: FrameCap, write_ms: f32, bytes: usize, frame_dt: f32) {
        match cap {
            FrameCap::Fps30 => self.fps = 30.0,
            FrameCap::Fps60 => self.fps = 60.0,
            FrameCap::Auto => {
                if write_ms > 8.0 || bytes > 150_000 {
                    self.fps = if self.fps > 50.0 { 45.0 } else { 30.0 };
                    self.good_for = 0.0;
                } else if write_ms < 3.0 && bytes < 60_000 {
                    self.good_for += frame_dt;
                    if self.good_for > 2.0 && self.fps < 60.0 {
                        self.fps = if self.fps < 40.0 { 45.0 } else { 60.0 };
                        self.good_for = 0.0;
                    }
                }
            }
        }
    }
}

/// Run the game until quit (or for `max_frames` in headless mode).
pub fn run(
    backend: &mut dyn Backend,
    mut ctx: Ctx,
    first: Box<dyn Screen>,
    max_frames: Option<u64>,
) -> anyhow::Result<RunReport> {
    let detected_color = ctx.caps.color;
    backend.set_color_mode(effective_color(ctx.settings.color, detected_color));
    ctx.audio.set_master(ctx.settings.volume);
    ctx.audio.start_ambience();

    let mut stack: Vec<Box<dyn Screen>> = vec![first];
    let mut input = InputState::default();
    let (w, h) = backend.size();
    let mut grid = CellGrid::new(w, h);
    let mut acc = 0.0f32;
    let mut last = Instant::now();
    let mut limiter = Limiter { fps: 60.0, good_for: 0.0 };
    let mut report = RunReport { frames: 0, render_ms: Vec::new(), bytes: Vec::new(), last_frame: None, summary: String::new() };
    let headless = ctx.opts.headless;

    loop {
        let frame_start = Instant::now();
        backend.poll(&mut input)?;
        if input.quit || ctx.quit || stack.is_empty() {
            break;
        }
        if input.resized {
            input.resized = false;
            let (w, h) = backend.size();
            grid.resize(w, h);
            backend.invalidate();
        }
        // Global keys.
        if input.was_pressed(Key::F(1)) {
            ctx.debug = !ctx.debug;
        }
        if input.was_pressed(Key::F(5)) {
            ctx.reload_assets();
        }

        let frame_dt = if headless { TICK } else { (frame_start - last).as_secs_f32().min(0.25) };
        last = frame_start;
        acc += frame_dt;

        let too_small = !ctx.opts.allow_small && (grid.w < MIN_W || grid.h < MIN_H);
        if too_small && input.was_pressed(Key::Esc) {
            break;
        }
        let mut steps = 0;
        while acc >= TICK - 1e-6 && steps < 5 {
            acc -= TICK;
            steps += 1;
            ctx.time += TICK as f64;
            for t in &mut ctx.toasts {
                t.ttl -= TICK;
            }
            ctx.toasts.retain(|t| t.ttl > 0.0);
            if too_small {
                input.clear_edges();
                continue;
            }
            let Some(top) = stack.last_mut() else { break };
            let tr = top.update(&mut ctx, &input, TICK);
            input.clear_edges();
            match tr {
                Transition::None => {}
                Transition::Push(s) => stack.push(s),
                Transition::Pop => {
                    stack.pop();
                    if let Some(t) = stack.last_mut() {
                        t.resumed(&mut ctx);
                    }
                }
                Transition::Replace(s) => {
                    stack.pop();
                    stack.push(s);
                }
                Transition::Reset(s) => {
                    stack.clear();
                    stack.push(s);
                }
                Transition::Quit => ctx.quit = true,
            }
            if ctx.quit || stack.is_empty() {
                break;
            }
        }
        if steps == 5 {
            acc = 0.0;
        }
        if ctx.settings_dirty {
            ctx.settings_dirty = false;
            backend.set_color_mode(effective_color(ctx.settings.color, detected_color));
            ctx.audio.set_master(ctx.settings.volume);
            let _ = ctx.settings.save();
        }
        if ctx.quit || stack.is_empty() {
            break;
        }

        // Render.
        let t_render = Instant::now();
        grid.clear();
        if too_small {
            debug::draw_too_small(&mut grid, &ctx);
        } else {
            let base = stack.iter().rposition(|s| !s.overlay()).unwrap_or(0);
            for s in stack[base..].iter_mut() {
                s.render(&mut ctx, &mut grid, &input);
            }
        }
        debug::draw_toasts(&mut grid, &ctx);
        let render_ms = t_render.elapsed().as_secs_f32() * 1000.0;
        if ctx.debug || ctx.settings.show_fps {
            debug::draw_overlay(&mut grid, &ctx, limiter.fps, ctx.debug);
        }
        let stats = backend.present(&grid)?;
        ctx.stats.record(frame_dt, render_ms, stats);
        report.frames += 1;
        if headless {
            report.render_ms.push(render_ms);
            report.bytes.push(stats.bytes);
        }
        if let Some(max) = max_frames {
            if report.frames >= max {
                report.last_frame = Some(grid.clone());
                break;
            }
        }
        if !headless {
            limiter.adapt(ctx.settings.frame_cap, stats.write_us as f32 / 1000.0, stats.bytes, frame_dt);
            let deadline = frame_start + Duration::from_secs_f32(1.0 / limiter.fps);
            backend.wait_until(deadline)?;
        }
    }
    report.last_frame.get_or_insert(grid);
    report.summary = ctx.run_summary.clone();
    Ok(report)
}
