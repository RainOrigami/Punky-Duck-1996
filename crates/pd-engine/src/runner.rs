//! Glue shared by the frontends: owns the running [`App`], the framebuffer
//! and the CD-music player, and executes [`Command`]s.
//!
//! The frontend still owns the loop and the clock. Per display frame (or
//! timer tick) it calls [`Runner::tick`] with the elapsed time and the input
//! snapshot, then [`Runner::render`]; whenever the audio device needs samples
//! it calls [`Runner::mix_audio`].

use std::rc::Rc;

use pd_formats::disc::Disc;

use crate::audio::CdPlayer;
use crate::{App, Command, Framebuffer, Input};

/// Longest step passed to `App::update` at once. Longer gaps (tab in the
/// background, window dragged, debugger) are dropped rather than replayed.
pub const MAX_STEP_US: u64 = 250_000;

/// The original's simulation tick: PIT channel 0 with divisor `0x4295`,
/// i.e. `1193182 / 17045` = 70.0019 Hz (`docs/re/engine/timing.md`). The
/// 640x480 video mode refreshes at practically the same rate (70.007 Hz in
/// the DOSBox-X capture), and movies and fades are paced on that retrace.
pub const TICK_PIT_HZ: u64 = 1_193_182;
pub const TICK_PIT_DIVISOR: u64 = 0x4295;
/// Tick length rounded to microseconds (use [`FixedStep::original`] for the
/// exact rate).
pub const ORIGINAL_TICK_US: u64 = 14_285;

/// Converts the variable `dt_us` an [`App`] receives into a whole number of
/// fixed logic ticks, carrying the remainder. The frontends tick at whatever
/// rate the display or timer gives (60, 144, 165 Hz, a 4 ms timer, ...); game
/// logic ported from the original runs at 70 Hz regardless.
///
/// The step is a rational number of microseconds (`num / den`), so the
/// original's tick is represented exactly and does not drift.
#[derive(Clone, Copy, Debug)]
pub struct FixedStep {
    /// Step length = `num / den` microseconds.
    num: u64,
    den: u64,
    /// Accumulated time in units of `1 / den` microseconds.
    acc: u64,
}

impl FixedStep {
    /// A step of `step_us` microseconds.
    pub fn new(step_us: u64) -> Self {
        Self { num: step_us.max(1), den: 1, acc: 0 }
    }

    /// The original's 70 Hz timer tick, exact.
    pub fn original() -> Self {
        Self { num: TICK_PIT_DIVISOR * 1_000_000, den: TICK_PIT_HZ, acc: 0 }
    }

    /// Add `dt_us` of elapsed time; returns how many ticks are now due
    /// (the original's `g_ticks` since the previous call).
    pub fn advance(&mut self, dt_us: u64) -> u32 {
        self.acc += dt_us * self.den;
        let steps = self.acc / self.num;
        self.acc -= steps * self.num;
        steps.min(u32::MAX as u64) as u32
    }

    /// Fraction of the next tick already elapsed, `0.0..1.0`.
    pub fn phase(&self) -> f32 {
        self.acc as f32 / self.num as f32
    }
}

/// Simulation steps the original runs for one rendered frame
/// (`logic_substeps`, `0x28d50`): `n = elapsed_ticks + speed_adjust`, at
/// least 1, and then `n + 1` iterations (the loop condition is `<=`).
/// `speed_adjust` is `GAME.STP[7]`, default 0.
///
/// How ticks are grouped into rendered frames decides the original's game
/// speed (it rendered uncapped, a few ticks per frame on period hardware), so
/// an `App` that wants the original feel picks a frame length in ticks and
/// calls this once per such frame; see `docs/re/engine/timing.md`.
pub fn original_substeps(elapsed_ticks: u32, speed_adjust: i32) -> u32 {
    let n = (elapsed_ticks as i64 + speed_adjust as i64).max(1);
    (n + 1).min(u32::MAX as i64) as u32
}

pub struct Runner {
    app: Box<dyn App>,
    fb: Framebuffer,
    cd: CdPlayer,
    commands: Vec<Command>,
    quit: bool,
}

impl Runner {
    /// `disc` supplies the CD audio; pass `None` when running from an
    /// extracted directory (music commands are then ignored).
    pub fn new(app: Box<dyn App>, disc: Option<Rc<Disc>>) -> Self {
        Self { app, fb: Framebuffer::new(), cd: CdPlayer::new(disc), commands: Vec::new(), quit: false }
    }

    /// Advance the app by `dt_us` microseconds (clamped to [`MAX_STEP_US`])
    /// and execute the commands it issued.
    pub fn tick(&mut self, dt_us: u64, input: &Input) {
        self.app.cd_status(self.cd.is_playing());
        self.app.update(dt_us.min(MAX_STEP_US), input, &mut self.commands);
        for cmd in self.commands.drain(..) {
            match cmd {
                Command::PlayCdTrack { track, looping } => {
                    self.cd.play(track, looping);
                }
                Command::StopCd => self.cd.stop(),
                Command::Quit => self.quit = true,
                Command::SetCdVolume { volume } => {
                    self.cd.set_volume((volume as u16 * crate::audio::VOLUME_UNITY).div_ceil(255));
                }
            }
        }
    }

    /// Let the app draw and return the framebuffer to present.
    pub fn render(&mut self) -> &Framebuffer {
        self.app.render(&mut self.fb);
        &self.fb
    }

    /// Fill `out` (interleaved stereo i16 at `SAMPLE_RATE`) with the app's
    /// sound effects plus CD music.
    pub fn mix_audio(&mut self, out: &mut [i16]) {
        self.app.mix_audio(out);
        self.cd.mix_into(out);
    }

    /// True once the app issued `Command::Quit`.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    pub fn cd(&self) -> &CdPlayer {
        &self.cd
    }

    pub fn cd_mut(&mut self) -> &mut CdPlayer {
        &mut self.cd
    }

    /// Forward a key press or release to the app (`App::key_event`).
    pub fn key_event(&mut self, scancode: u8, pressed: bool) {
        self.app.key_event(scancode, pressed);
    }

    /// Forward relative mouse movement to the app (`App::mouse_motion`).
    pub fn mouse_motion(&mut self, dx: f32, dy: f32) {
        self.app.mouse_motion(dx, dy);
    }

    /// `App::wants_pointer_lock`.
    pub fn wants_pointer_lock(&self) -> bool {
        self.app.wants_pointer_lock()
    }

    pub fn app_mut(&mut self) -> &mut dyn App {
        self.app.as_mut()
    }

    /// Replace the running app (e.g. intro -> game). CD music keeps playing.
    pub fn set_app(&mut self, app: Box<dyn App>) {
        self.app = app;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_step_is_rate_independent() {
        // Ten seconds delivered at 60 Hz, 165 Hz or in 4 ms timer ticks give
        // the same number of 70 Hz ticks.
        for dt in [16_667u64, 6_061, 4_000] {
            let mut fs = FixedStep::original();
            let calls = 10_000_000 / dt;
            let ticks: u32 = (0..calls).map(|_| fs.advance(dt)).sum::<u32>() + fs.advance(10_000_000 - calls * dt);
            assert_eq!(ticks, 700, "dt {dt}");
        }
        // Exact rate: one hour is 1193182 * 3600 / 17045 ticks.
        let mut fs = FixedStep::original();
        let ticks: u64 = (0..3600).map(|_| fs.advance(1_000_000) as u64).sum();
        assert_eq!(ticks, 1_193_182 * 3600 / 17_045);

        let mut fs = FixedStep::new(1000);
        assert_eq!(fs.advance(2500), 2);
        assert!((fs.phase() - 0.5).abs() < 1e-6);
        assert_eq!(fs.advance(500), 1);
    }

    #[test]
    fn substeps_follow_the_original_loop() {
        assert_eq!(original_substeps(0, 0), 2); // waits for retrace, n = 1, runs 2
        assert_eq!(original_substeps(1, 0), 2);
        assert_eq!(original_substeps(4, 0), 5);
        assert_eq!(original_substeps(4, -2), 3);
        assert_eq!(original_substeps(1, -5), 2);
    }

    struct Probe(Vec<u64>);
    impl App for Probe {
        fn update(&mut self, dt_us: u64, _input: &Input, commands: &mut Vec<Command>) {
            self.0.push(dt_us);
            commands.push(Command::PlayCdTrack { track: 2, looping: true });
            commands.push(Command::Quit);
        }
        fn render(&mut self, fb: &mut Framebuffer) {
            fb.pixels[0] = 7;
        }
        fn mix_audio(&mut self, out: &mut [i16]) {
            out.fill(3);
        }
    }

    #[test]
    fn runner_drives_app_and_commands() {
        let mut r = Runner::new(Box::new(Probe(Vec::new())), None);
        assert!(!r.quit_requested());
        r.tick(10_000_000, &Input::default());
        assert!(r.quit_requested());
        assert!(!r.cd().is_playing(), "no disc: music commands are ignored");
        assert_eq!(r.render().pixels[0], 7);
        let mut out = [0i16; 4];
        r.mix_audio(&mut out);
        assert_eq!(out, [3; 4]);
    }
}
