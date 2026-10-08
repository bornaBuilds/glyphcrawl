//! F1 debug overlay, toasts and the "terminal too small" screen.

use super::{Ctx, MIN_H, MIN_W};
use crate::platform::PresentStats;
use crate::render::cell::{Cell, CellGrid, Rgb};

#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    pub fps: f32,
    pub render_ms: f32,
    pub bytes: f32,
    pub write_ms: f32,
    pub max_bytes: usize,
}

impl FrameStats {
    pub fn record(&mut self, dt: f32, render_ms: f32, p: PresentStats) {
        let a = 0.1;
        if dt > 0.0 {
            self.fps += (1.0 / dt - self.fps) * a;
        }
        self.render_ms += (render_ms - self.render_ms) * a;
        self.bytes += (p.bytes as f32 - self.bytes) * a;
        self.write_ms += (p.write_us as f32 / 1000.0 - self.write_ms) * a;
        self.max_bytes = self.max_bytes.max(p.bytes);
    }
}

pub fn draw_overlay(grid: &mut CellGrid, ctx: &Ctx, cap: f32, full: bool) {
    let s = &ctx.stats;
    let line = if full {
        format!(
            " {:>4.0} fps (cap {:.0}) | render {:>5.2} ms | out {:>6.1} KB/f, {:>5.2} ms | {} {:?} keys:{} | {} ",
            s.fps,
            cap,
            s.render_ms,
            s.bytes / 1024.0,
            s.write_ms,
            ctx.caps.name,
            ctx.caps.color,
            if ctx.caps.key_release { "release" } else { "press-only" },
            ctx.debug_line
        )
    } else {
        format!(" {:.0} fps ", s.fps)
    };
    let x = grid.w as i32 - line.chars().count() as i32;
    grid.text_bg(x.max(0), 0, &line, Rgb(200, 255, 200), Rgb(20, 40, 20));
}

pub fn draw_toasts(grid: &mut CellGrid, ctx: &Ctx) {
    let mut y = grid.h as i32 - 5;
    for t in ctx.toasts.iter().rev().take(4) {
        let fade = t.ttl.min(1.0);
        let text = format!(" {} ", t.text);
        let x = (grid.w as i32 - text.chars().count() as i32) / 2;
        grid.text_bg(x, y, &text, t.color.scale(fade), Rgb(20, 20, 28));
        y -= 1;
    }
}

pub fn draw_too_small(grid: &mut CellGrid, ctx: &Ctx) {
    grid.fill(0, 0, grid.w as i32, grid.h as i32, Cell::BLANK);
    let h = grid.h as i32;
    let lines = [
        format!("Terminal is {}x{} — glyphcrawl needs at least {}x{}.", grid.w, grid.h, MIN_W, MIN_H),
        if ctx.caps.font_zoom {
            "Scroll the mouse wheel down (smaller font) or enlarge the window.".to_string()
        } else {
            "Zoom out with your terminal's font shortcut (Ctrl+- / Ctrl+Shift+-) or enlarge it.".to_string()
        },
        "Ctrl+C or Esc to quit.".to_string(),
    ];
    for (i, l) in lines.iter().enumerate() {
        grid.text_centered(h / 2 - 1 + i as i32, l, Rgb(230, 200, 120));
    }
}
