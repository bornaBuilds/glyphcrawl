//! Sound. With the `audio` feature, effects are synthesised in code at startup and played
//! through kira; without it (or with no sound device) everything is a silent no-op.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sfx {
    Swing,
    Hit,
    Crit,
    Hurt,
    Block,
    Bolt,
    Fireball,
    Explosion,
    Frost,
    Blink,
    Dash,
    Arrow,
    Knife,
    Smoke,
    EnemyDie,
    Slime,
    Bat,
    Slam,
    DoorSlam,
    DoorOpen,
    Pickup,
    Upgrade,
    UiMove,
    UiSelect,
    HeroDie,
    Victory,
}

pub const ALL_SFX: [Sfx; 26] = [
    Sfx::Swing, Sfx::Hit, Sfx::Crit, Sfx::Hurt, Sfx::Block, Sfx::Bolt, Sfx::Fireball, Sfx::Explosion,
    Sfx::Frost, Sfx::Blink, Sfx::Dash, Sfx::Arrow, Sfx::Knife, Sfx::Smoke, Sfx::EnemyDie, Sfx::Slime,
    Sfx::Bat, Sfx::Slam, Sfx::DoorSlam, Sfx::DoorOpen, Sfx::Pickup, Sfx::Upgrade, Sfx::UiMove,
    Sfx::UiSelect, Sfx::HeroDie, Sfx::Victory,
];

impl Sfx {
    pub fn from_name(s: &str) -> Option<Sfx> {
        ALL_SFX.iter().copied().find(|x| format!("{x:?}").eq_ignore_ascii_case(s))
    }
}

pub trait AudioOut {
    fn play(&mut self, sfx: Sfx, volume: f32);
    fn set_master(&mut self, volume: f32);
    fn start_ambience(&mut self) {}
    fn is_real(&self) -> bool {
        false
    }
}

pub struct NullAudio;

impl AudioOut for NullAudio {
    fn play(&mut self, _sfx: Sfx, _volume: f32) {}
    fn set_master(&mut self, _volume: f32) {}
}

/// Create the best available audio output.
pub fn create(muted: bool) -> Box<dyn AudioOut> {
    if muted {
        return Box::new(NullAudio);
    }
    #[cfg(feature = "audio")]
    {
        match kira_out::KiraAudio::new() {
            Ok(a) => return Box::new(a),
            Err(e) => eprintln!("audio disabled: {e}"),
        }
    }
    Box::new(NullAudio)
}

/// Procedural sound synthesis (sfxr-style), shared by the kira backend and tests.
pub mod synth {
    use super::Sfx;

    pub const RATE: u32 = 44_100;

    struct Noise(u32);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    /// One voice: frequency sweep + noise mix + envelope.
    struct Voice {
        dur: f32,
        f0: f32,
        f1: f32,
        noise: f32,
        square: f32,
        attack: f32,
        gain: f32,
        lowpass: f32,
    }

    fn render(voices: &[Voice]) -> Vec<f32> {
        let dur = voices.iter().map(|v| v.dur).fold(0.0, f32::max);
        let n = (dur * RATE as f32) as usize;
        let mut out = vec![0.0f32; n];
        let mut noise = Noise(0x9e37_79b9);
        for v in voices {
            let len = (v.dur * RATE as f32) as usize;
            let mut phase = 0.0f32;
            let mut lp = 0.0f32;
            for (i, o) in out.iter_mut().enumerate().take(len) {
                let t = i as f32 / RATE as f32;
                let k = t / v.dur;
                let f = v.f0 * (v.f1 / v.f0).powf(k);
                phase += f / RATE as f32;
                let s = (phase * std::f32::consts::TAU).sin();
                let sq = if s >= 0.0 { 1.0 } else { -1.0 };
                let tone = s * (1.0 - v.square) + sq * v.square;
                let raw = tone * (1.0 - v.noise) + noise.next() * v.noise;
                lp += (raw - lp) * v.lowpass;
                let env = if t < v.attack { t / v.attack.max(1e-4) } else { (1.0 - k).max(0.0).powf(1.6) };
                *o += lp * env * v.gain;
            }
        }
        out
    }

    fn v(dur: f32, f0: f32, f1: f32, noise: f32, gain: f32) -> Voice {
        Voice { dur, f0, f1, noise, square: 0.0, attack: 0.004, gain, lowpass: 1.0 }
    }

    pub fn samples(sfx: Sfx) -> Vec<f32> {
        let lp = |mut x: Voice, l: f32| {
            x.lowpass = l;
            x
        };
        let sq = |mut x: Voice, s: f32| {
            x.square = s;
            x
        };
        match sfx {
            Sfx::Swing => render(&[lp(v(0.16, 900.0, 300.0, 0.9, 0.5), 0.25)]),
            Sfx::Hit => render(&[lp(v(0.09, 220.0, 90.0, 0.6, 0.8), 0.5)]),
            Sfx::Crit => render(&[lp(v(0.12, 320.0, 80.0, 0.5, 0.9), 0.6), v(0.15, 1400.0, 900.0, 0.0, 0.25)]),
            Sfx::Hurt => render(&[sq(v(0.18, 300.0, 120.0, 0.3, 0.45), 0.5)]),
            Sfx::Block => render(&[sq(v(0.12, 1200.0, 900.0, 0.3, 0.35), 0.6), v(0.25, 2400.0, 2300.0, 0.0, 0.15)]),
            Sfx::Bolt => render(&[v(0.18, 500.0, 1500.0, 0.05, 0.35)]),
            Sfx::Fireball => render(&[lp(v(0.35, 200.0, 500.0, 0.7, 0.5), 0.2)]),
            Sfx::Explosion => render(&[lp(v(0.6, 120.0, 40.0, 0.85, 1.0), 0.12)]),
            Sfx::Frost => render(&[v(0.4, 2400.0, 1800.0, 0.4, 0.35), lp(v(0.4, 300.0, 200.0, 0.8, 0.3), 0.1)]),
            Sfx::Blink => render(&[v(0.2, 300.0, 2400.0, 0.1, 0.35)]),
            Sfx::Dash => render(&[lp(v(0.22, 400.0, 150.0, 0.95, 0.5), 0.15)]),
            Sfx::Arrow => render(&[lp(v(0.12, 1800.0, 600.0, 0.8, 0.4), 0.4)]),
            Sfx::Knife => render(&[lp(v(0.08, 2400.0, 1200.0, 0.7, 0.35), 0.6)]),
            Sfx::Smoke => render(&[lp(v(0.5, 300.0, 100.0, 1.0, 0.6), 0.06)]),
            Sfx::EnemyDie => render(&[sq(v(0.3, 400.0, 60.0, 0.4, 0.45), 0.3)]),
            Sfx::Slime => render(&[v(0.15, 150.0, 450.0, 0.0, 0.45)]),
            Sfx::Bat => render(&[sq(v(0.08, 2600.0, 3200.0, 0.1, 0.2), 0.5)]),
            Sfx::Slam => render(&[lp(v(0.7, 90.0, 30.0, 0.7, 1.1), 0.08)]),
            Sfx::DoorSlam => render(&[lp(v(0.45, 80.0, 40.0, 0.6, 1.0), 0.1), sq(v(0.1, 300.0, 200.0, 0.5, 0.3), 0.5)]),
            Sfx::DoorOpen => render(&[lp(v(0.5, 60.0, 90.0, 0.9, 0.6), 0.05)]),
            Sfx::Pickup => render(&[v(0.08, 880.0, 880.0, 0.0, 0.3), v(0.16, 1320.0, 1320.0, 0.0, 0.25)]),
            Sfx::Upgrade => render(&[v(0.5, 660.0, 1320.0, 0.0, 0.3), v(0.5, 990.0, 1980.0, 0.0, 0.2)]),
            Sfx::UiMove => render(&[sq(v(0.03, 1200.0, 1200.0, 0.0, 0.12), 0.8)]),
            Sfx::UiSelect => render(&[sq(v(0.08, 800.0, 1600.0, 0.0, 0.18), 0.8)]),
            Sfx::HeroDie => render(&[sq(v(1.2, 440.0, 55.0, 0.2, 0.5), 0.4)]),
            Sfx::Victory => render(&[v(0.25, 523.0, 523.0, 0.0, 0.3), v(0.5, 659.0, 659.0, 0.0, 0.25), v(0.9, 784.0, 784.0, 0.0, 0.25)]),
        }
    }

    /// Ten seconds of low dungeon rumble, seamless loop.
    pub fn ambience() -> Vec<f32> {
        let n = 10 * RATE as usize;
        let mut out = vec![0.0f32; n];
        let mut noise = Noise(12345);
        let (mut b0, mut b1) = (0.0f32, 0.0f32);
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            // Brown-ish noise.
            b0 = (b0 + noise.next() * 0.02).clamp(-1.0, 1.0) * 0.998;
            b1 += (b0 - b1) * 0.05;
            let lfo = 0.6 + 0.4 * (t * std::f32::consts::TAU / 10.0).sin();
            let drone = (t * 55.0 * std::f32::consts::TAU).sin() * 0.04 + (t * 82.5 * std::f32::consts::TAU).sin() * 0.025;
            *o = (b1 * 0.5 + drone) * lfo;
        }
        // Crossfade the ends so the loop has no click.
        let fade = RATE as usize / 2;
        for i in 0..fade {
            let k = i as f32 / fade as f32;
            let a = out[i];
            let b = out[n - fade + i];
            out[i] = a * k + b * (1.0 - k);
        }
        out.truncate(n - fade);
        out
    }
}

#[cfg(feature = "audio")]
mod kira_out {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
    use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

    use super::{synth, AudioOut, Sfx, ALL_SFX};

    pub struct KiraAudio {
        manager: AudioManager<DefaultBackend>,
        sounds: BTreeMap<Sfx, StaticSoundData>,
        ambience: StaticSoundData,
        master: f32,
    }

    fn to_data(samples: Vec<f32>, looped: bool) -> StaticSoundData {
        let frames: Arc<[Frame]> = samples.into_iter().map(|s| Frame { left: s, right: s }).collect();
        let mut settings = StaticSoundSettings::new();
        if looped {
            settings = settings.loop_region(..);
        }
        StaticSoundData { sample_rate: synth::RATE, frames, settings, slice: None }
    }

    fn db(v: f32) -> Decibels {
        if v <= 0.001 { Decibels::SILENCE } else { Decibels(20.0 * v.log10()) }
    }

    impl KiraAudio {
        pub fn new() -> anyhow::Result<KiraAudio> {
            let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let sounds = ALL_SFX.iter().map(|s| (*s, to_data(synth::samples(*s), false))).collect();
            Ok(KiraAudio { manager, sounds, ambience: to_data(synth::ambience(), true), master: 1.0 })
        }
    }

    impl AudioOut for KiraAudio {
        fn play(&mut self, sfx: Sfx, volume: f32) {
            if let Some(d) = self.sounds.get(&sfx) {
                let d = d.clone().volume(db(volume * self.master));
                let _ = self.manager.play(d);
            }
        }

        fn set_master(&mut self, volume: f32) {
            self.master = volume.clamp(0.0, 1.0);
            let _ = self.manager.main_track().set_volume(db(self.master), Tween::default());
        }

        fn start_ambience(&mut self) {
            let d = self.ambience.clone().volume(db(0.5));
            let _ = self.manager.play(d);
        }

        fn is_real(&self) -> bool {
            true
        }
    }
}
