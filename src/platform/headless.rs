//! Headless backend: no screen, no real input. Runs the full render + terminal-encode
//! pipeline so tests and the bot can measure bytes/frame and catch panics.

use std::time::Instant;

use super::term_writer::TermWriter;
use super::{Backend, Caps, ColorMode, InputState, PresentStats};
use crate::render::cell::CellGrid;

pub struct HeadlessBackend {
    size: (u16, u16),
    writer: TermWriter,
    pub last_frame: Option<CellGrid>,
    pub keep_frames: bool,
}

impl HeadlessBackend {
    pub fn new(w: u16, h: u16) -> HeadlessBackend {
        HeadlessBackend { size: (w, h), writer: TermWriter::new(ColorMode::TrueColor), last_frame: None, keep_frames: true }
    }
}

impl Backend for HeadlessBackend {
    fn caps(&self) -> Caps {
        Caps { key_release: true, color: ColorMode::TrueColor, name: "headless", font_zoom: false }
    }

    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn poll(&mut self, _input: &mut InputState) -> anyhow::Result<()> {
        Ok(())
    }

    fn present(&mut self, grid: &CellGrid) -> anyhow::Result<PresentStats> {
        let t0 = Instant::now();
        let n = self.writer.encode(grid).len();
        if self.keep_frames {
            self.last_frame = Some(grid.clone());
        }
        Ok(PresentStats { bytes: n, write_us: t0.elapsed().as_micros() as u64 })
    }

    fn wait_until(&mut self, _deadline: Instant) -> anyhow::Result<()> {
        Ok(())
    }

    fn invalidate(&mut self) {
        self.writer.invalidate();
    }
}
