use std::path::PathBuf;

use clap::Parser;

use glyphcrawl::app::settings::{InputMode, Settings};
use glyphcrawl::app::{self, Ctx, Options, Screen};
use glyphcrawl::assets::{self, Assets};
use glyphcrawl::platform::{self, audio, Backend};

#[derive(Parser, Debug)]
#[command(name = "glyphcrawl", version, about = "An isometric ASCII dungeon crawler")]
struct Cli {
    /// Open a native window instead of using the terminal.
    #[arg(long)]
    window: bool,
    /// Run without any screen (for tests / benchmarking).
    #[arg(long)]
    headless: bool,
    /// Headless: number of frames to run.
    #[arg(long, default_value_t = 600)]
    frames: u64,
    /// Headless: grid size as WxH.
    #[arg(long, default_value = "200x60")]
    size: String,
    /// Headless: print the last frame as text.
    #[arg(long)]
    dump: bool,
    /// Headless: save the last frame as a PNG screenshot.
    #[arg(long)]
    png: Option<PathBuf>,
    /// Dungeon seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Start a run straight away with this class (knight, wizard, rogue, archer).
    #[arg(long)]
    class: Option<String>,
    /// Start in a single room template (e.g. "test").
    #[arg(long)]
    room: Option<String>,
    /// Hero can't die.
    #[arg(long)]
    god: bool,
    /// Let the autoplayer drive the hero.
    #[arg(long)]
    bot: bool,
    /// No sound.
    #[arg(long)]
    mute: bool,
    /// Ignore key-release support and use mouse-move mode.
    #[arg(long)]
    mouse_only: bool,
    /// Which screen to open first: menu, game, gallery, sandbox.
    #[arg(long, default_value = "menu")]
    screen: String,
    /// Assets directory.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// Window font size in pixels.
    #[arg(long, default_value_t = 14.0)]
    font_px: f32,
    /// Don't stop at the minimum terminal size.
    #[arg(long)]
    allow_small: bool,
}

fn first_screen(name: &str, ctx: &mut Ctx) -> anyhow::Result<Box<dyn Screen>> {
    let seed = ctx.opts.seed.unwrap_or(1);
    let s: Box<dyn Screen> = match name {
        "sandbox" => Box::new(app::screens::sandbox::Sandbox::new()),
        "game" => {
            let room = ctx.opts.room.clone().unwrap_or_else(|| "test".into());
            Box::new(app::screens::game::GameScreen::from_room(ctx, &room, seed)?)
        }
        other => anyhow::bail!("unknown screen '{other}'"),
    };
    Ok(s)
}

#[cfg(feature = "window")]
fn save_png(path: &std::path::Path, grid: &glyphcrawl::render::cell::CellGrid, font: &[u8]) -> anyhow::Result<()> {
    let mut r = platform::glyph_atlas::GlyphRaster::new(font, 12.0)?;
    let (w, h, px) = r.render_grid(grid);
    platform::glyph_atlas::write_png(path, w, h, &px)?;
    Ok(())
}

#[cfg(not(feature = "window"))]
fn save_png(_: &std::path::Path, _: &glyphcrawl::render::cell::CellGrid, _: &[u8]) -> anyhow::Result<()> {
    anyhow::bail!("--png needs the `window` feature")
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let assets_dir = assets::find_dir(cli.assets.as_deref())?;
    let assets = Assets::load(&assets_dir)?;
    let font = assets.font.clone();
    let mut settings = Settings::load();
    if cli.mouse_only {
        settings.input_mode = InputMode::MouseOnly;
    }
    let opts = Options {
        seed: cli.seed,
        class: cli.class.clone(),
        room: cli.room.clone(),
        god: cli.god,
        bot: cli.bot,
        headless: cli.headless,
        allow_small: cli.allow_small || cli.headless,
    };

    let mut backend: Box<dyn Backend> = if cli.headless {
        let (w, h) = cli
            .size
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            .ok_or_else(|| anyhow::anyhow!("--size must look like 200x60"))?;
        Box::new(platform::headless::HeadlessBackend::new(w, h))
    } else if cli.window {
        #[cfg(feature = "window")]
        {
            Box::new(platform::window::WindowBackend::new(&assets.font, cli.font_px)?)
        }
        #[cfg(not(feature = "window"))]
        anyhow::bail!("built without the `window` feature")
    } else {
        #[cfg(feature = "terminal")]
        {
            Box::new(platform::terminal::TerminalBackend::new(
                platform::detect_color_mode(),
                settings.input_mode == InputMode::MouseOnly,
            )?)
        }
        #[cfg(not(feature = "terminal"))]
        anyhow::bail!("built without the `terminal` feature")
    };

    let audio = audio::create(cli.mute || cli.headless);
    let mut ctx = Ctx {
        assets,
        assets_dir,
        caps: backend.caps(),
        settings,
        audio,
        opts,
        time: 0.0,
        toasts: Vec::new(),
        debug: false,
        debug_line: String::new(),
        stats: Default::default(),
        quit: false,
        settings_dirty: false,
    };
    let first = first_screen(&cli.screen, &mut ctx)?;
    let max_frames = if cli.headless { Some(cli.frames) } else { None };
    let report = app::run(backend.as_mut(), ctx, first, max_frames);
    drop(backend);
    let report = report?;

    if cli.headless {
        let n = report.render_ms.len().max(1) as f32;
        let avg = report.render_ms.iter().sum::<f32>() / n;
        let mut sorted = report.render_ms.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let p99 = sorted.get((sorted.len() as f32 * 0.99) as usize).copied().unwrap_or(0.0);
        let avg_b = report.bytes.iter().sum::<usize>() as f32 / n;
        let max_b = report.bytes.iter().max().copied().unwrap_or(0);
        if cli.dump {
            if let Some(g) = &report.last_frame {
                print!("{}", g.to_text());
            }
        }
        if let (Some(path), Some(g)) = (&cli.png, &report.last_frame) {
            save_png(path, g, &font)?;
        }
        println!(
            "frames {} | render avg {:.2} ms p99 {:.2} ms | bytes/frame avg {:.0} max {}",
            report.frames, avg, p99, avg_b, max_b
        );
    }
    Ok(())
}
