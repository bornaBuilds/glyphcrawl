//! Terminal backend (crossterm): raw mode, alternate screen, mouse capture, the kitty
//! keyboard protocol for key releases, and diffed output through `TermWriter`.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
    KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{cursor, execute, terminal};

use super::term_writer::TermWriter;
use super::{Backend, Caps, ColorMode, InputState, Key, MouseButton, PresentStats};
use crate::render::cell::CellGrid;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static KEYBOARD_PUSHED: AtomicBool = AtomicBool::new(false);

/// Put the terminal back the way we found it. Safe to call more than once (Drop + panic hook).
pub fn restore_terminal() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = std::io::stdout();
    if KEYBOARD_PUSHED.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableMouseCapture,
        DisableFocusChange,
        terminal::EnableLineWrap,
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = out.write_all(b"\x1b[0m");
    let _ = out.flush();
    let _ = terminal::disable_raw_mode();
}

pub struct TerminalBackend {
    writer: TermWriter,
    key_release: bool,
    size: (u16, u16),
}

impl TerminalBackend {
    pub fn new(color: ColorMode, force_mouse_only: bool) -> anyhow::Result<TerminalBackend> {
        terminal::enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);

        // Make sure a panic message is readable: restore first, then print.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            default_hook(info);
        }));

        let mut out = std::io::stdout();
        execute!(
            out,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::DisableLineWrap,
            EnableMouseCapture,
            EnableFocusChange
        )?;

        let supported = !force_mouse_only && terminal::supports_keyboard_enhancement().unwrap_or(false);
        if supported {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                )
            )?;
            KEYBOARD_PUSHED.store(true, Ordering::SeqCst);
        }

        let size = terminal::size()?;
        Ok(TerminalBackend { writer: TermWriter::new(color), key_release: supported, size })
    }
}

impl Drop for TerminalBackend {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn map_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(c) => Key::Char(c.to_ascii_lowercase()),
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab | KeyCode::BackTab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    })
}

fn map_button(b: event::MouseButton) -> MouseButton {
    match b {
        event::MouseButton::Left => MouseButton::Left,
        event::MouseButton::Right => MouseButton::Right,
        event::MouseButton::Middle => MouseButton::Middle,
    }
}

impl Backend for TerminalBackend {
    fn caps(&self) -> Caps {
        Caps { key_release: self.key_release, color: self.writer.mode, name: "terminal", font_zoom: false }
    }

    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn poll(&mut self, input: &mut InputState) -> anyhow::Result<()> {
        let now = Instant::now();
        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(k) => {
                    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                        input.quit = true;
                        continue;
                    }
                    let Some(key) = map_key(k.code) else { continue };
                    if self.key_release {
                        match k.kind {
                            KeyEventKind::Press => input.key_down(key),
                            KeyEventKind::Repeat => {}
                            KeyEventKind::Release => input.key_up(key),
                        }
                    } else if k.kind != KeyEventKind::Release {
                        // No release events: a press holds the key for a moment; auto-repeat
                        // keeps refreshing it. The first press must outlast the repeat delay.
                        let ttl = if input.held.contains(&key) { 0.12 } else { 0.55 };
                        input.key_pulse(key, now + Duration::from_secs_f32(ttl));
                    }
                }
                Event::Mouse(m) => {
                    input.mouse = Some((m.column as f32 + 0.5, m.row as f32 + 0.5));
                    match m.kind {
                        MouseEventKind::Down(b) => input.mouse_down(map_button(b)),
                        MouseEventKind::Up(b) => input.mouse_up(map_button(b)),
                        MouseEventKind::ScrollUp => input.wheel += 1,
                        MouseEventKind::ScrollDown => input.wheel -= 1,
                        _ => {}
                    }
                }
                Event::Resize(w, h) => {
                    self.size = (w, h);
                    input.resized = true;
                }
                Event::FocusLost => input.release_all(),
                _ => {}
            }
        }
        if !self.key_release {
            input.expire_pulses(now);
        }
        Ok(())
    }

    fn present(&mut self, grid: &CellGrid) -> anyhow::Result<PresentStats> {
        let t0 = Instant::now();
        let bytes = self.writer.encode(grid);
        let n = bytes.len();
        if n > 0 {
            let mut out = std::io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()?;
        }
        Ok(PresentStats { bytes: n, write_us: t0.elapsed().as_micros() as u64 })
    }

    fn wait_until(&mut self, deadline: Instant) -> anyhow::Result<()> {
        let now = Instant::now();
        if deadline > now {
            // Wakes early if input arrives; the event stays queued for the next poll.
            event::poll(deadline - now)?;
        }
        Ok(())
    }

    fn invalidate(&mut self) {
        self.writer.invalidate();
    }

    fn set_color_mode(&mut self, mode: ColorMode) {
        if self.writer.mode != mode {
            self.writer.mode = mode;
            self.writer.invalidate();
        }
    }
}
