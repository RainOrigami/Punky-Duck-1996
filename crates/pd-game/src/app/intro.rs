//! `INTRO.EXE`: the three movies before the game (`0x103d0`, player `0x10010`).
//!
//! ```text
//! play("C_SOFT.MOV", hold 200 ms, CD track 2)
//! play("PUNKY.MOV",  hold 1000 ms, no CD change)
//! if not skipped: play("INTRO.MOV", hold 200 ms, CD track 3)
//! cd_stop
//! ```
//!
//! Unlike `flc_play` in `P.EXE` this player runs on a 1 kHz timer (PIT
//! divisor `0x4a9`, `0x11370`): frame `k` is due `speed` ms after frame
//! `k - 1`, frames that are already late are decoded but not shown, the
//! first frame's palette is faded in over 33 retraces, and after the last
//! frame the picture is held and faded out over 101 retraces. A key in the
//! DOS buffer ends a movie after its current frame; the key is never read,
//! so one key press cuts all three movies short. Addresses are `INTRO.elf`'s.

use pd_engine::runner::FixedStep;
use pd_formats::flc::Flc;

use super::core::Core;
use super::gfx::{self, BLACK, Fade, Pal};

struct MovieSpec {
    name: &'static str,
    hold_ms: u64,
    track: Option<u8>,
}

const MOVIES: [MovieSpec; 3] = [
    MovieSpec { name: "C_SOFT.MOV", hold_ms: 200, track: Some(2) },
    MovieSpec { name: "PUNKY.MOV", hold_ms: 1000, track: None },
    MovieSpec { name: "INTRO.MOV", hold_ms: 200, track: Some(3) },
];

/// Frame-speed changes of `INTRO.MOV` (table `0x6ba90`, filled at
/// `0x1003b`): frame index and retrace count; the speed becomes
/// `trunc(count * 14.28)` ms from that frame on.
const INTRO_SPEEDS: [(u32, u32); 3] = [(0, 8), (416, 7), (441, 8)];

enum State {
    /// Open movie `n`.
    Open(usize),
    /// Frame decoded, waiting for its due time.
    Frame,
    /// First frame: palette fade-in (33 retraces), then the blit.
    FadeIn(Fade),
    /// After the last frame: `delay(hold)`.
    Hold(u64),
    /// `palette_fade(1, 255, 100, movie palette -> black)`.
    FadeOut(Fade),
    Done,
}

struct Playing {
    spec: usize,
    flc: Flc<Vec<u8>>,
    /// Index of the frame that is decoded and waiting (10000 after a key).
    i: u32,
    /// Frames decoded so far (the player's own counter, `+8`).
    decoded: u32,
    speed_ms: u64,
    /// Time of the schedule's last frame (`+0x61a`), in microseconds.
    last_us: u64,
    /// This frame was already late when it was decoded (`+0x622`): not shown.
    late: bool,
    /// Palette changed in a frame not yet shown (`+0x614`).
    pal_changed: bool,
    pal: Pal,
}

pub struct Intro {
    state: State,
    cur: Option<Playing>,
    now_us: u64,
    retrace: FixedStep,
    /// `0x20124`: a key ended a movie.
    skipped: bool,
}

impl Default for Intro {
    fn default() -> Self {
        Self::new()
    }
}

impl Intro {
    pub fn new() -> Self {
        Self { state: State::Open(0), cur: None, now_us: 0, retrace: FixedStep::original(), skipped: false }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// Name and frame of the movie on screen, for traces.
    pub fn position(&self) -> Option<(&'static str, u32)> {
        self.cur.as_ref().map(|p| (MOVIES[p.spec].name, p.decoded))
    }

    fn finish(&mut self, core: &mut Core) {
        // 0x104e0: palette to black unless skipped, CD stop, text mode.
        if !self.skipped {
            core.video.set_palette(&BLACK);
        }
        core.cd_stop();
        core.video.fill_screen(0);
        core.video.set_palette(&BLACK);
        self.cur = None;
        self.state = State::Done;
    }

    fn open(&mut self, core: &mut Core, n: usize) {
        // INTRO.MOV is only played when no key ended an earlier movie.
        if n >= MOVIES.len() || (n == 2 && self.skipped) {
            return self.finish(core);
        }
        if n == 1 {
            // 0x1045a: the flag is cleared after C_SOFT.MOV.
            self.skipped = false;
        }
        let spec = &MOVIES[n];
        let Some(flc) = core.assets.load(spec.name).and_then(|d| Flc::new(d).ok()) else {
            core.errors.push(format!("{}: cannot play", spec.name));
            self.state = State::Open(n + 1);
            return;
        };
        core.note(format!("movie {}", spec.name));
        let speed_ms = (flc.header().speed_ms & 0xffff) as u64;
        self.cur = Some(Playing {
            spec: n,
            flc,
            i: 0,
            decoded: 0,
            speed_ms,
            last_us: self.now_us,
            late: false,
            pal_changed: false,
            pal: BLACK,
        });
        self.begin_frame(core);
    }

    /// Top of the frame loop (`0x100ab`): CD start, key test, decode.
    fn begin_frame(&mut self, core: &mut Core) {
        let Some(p) = self.cur.as_mut() else { return };
        let spec = &MOVIES[p.spec];
        if p.i == 0 {
            if let Some(track) = spec.track {
                // Stop and play, twice (0x100b9).
                core.cd_stop();
                core.cd_play(track);
            }
        }
        if core.kbd.kbhit() {
            p.i = 10_000;
            self.skipped = true;
        }
        let total = p.flc.header().frames as u32;
        if p.decoded < total {
            match p.flc.decode_next() {
                Ok(Some(info)) => {
                    // The flag survives a frame that was not shown (0x2c2a0 in P.EXE's copy).
                    p.pal_changed = info.palette_changed || (p.late && p.pal_changed);
                    if info.palette_changed {
                        for (dst, src) in p.pal.iter_mut().zip(p.flc.palette()) {
                            *dst = src.map(|c| c >> 2);
                        }
                    }
                }
                _ => p.i = 10_000,
            }
            p.decoded += 1;
        }
        if spec.name == "INTRO.MOV" {
            if let Some(&(_, n)) = INTRO_SPEEDS.iter().find(|&&(frame, _)| frame == p.i) {
                p.speed_ms = (n as f64 * 14.28) as u64;
            }
        }
        p.late = self.now_us.saturating_sub(p.last_us) > p.speed_ms * 1000;
        self.state = State::Frame;
    }

    fn blit(p: &Playing, core: &mut Core) {
        // Centred (0x1020f); all shipped movies are 640x480.
        let (w, h) = (p.flc.width().min(gfx::W), p.flc.height().min(gfx::H));
        let (x0, y0) = ((gfx::W - w) / 2, (gfx::H - h) / 2);
        for (y, row) in p.flc.pixels().chunks_exact(p.flc.width()).take(h).enumerate() {
            let at = (y0 + y) * gfx::W + x0;
            core.video.screen.pixels[at..at + w].copy_from_slice(&row[..w]);
        }
    }

    /// After a frame was shown or dropped: next frame or the end of the movie.
    fn advance(&mut self, core: &mut Core) {
        let Some(p) = self.cur.as_mut() else { return };
        p.i = p.i.saturating_add(1);
        if p.i < p.flc.header().frames as u32 {
            self.begin_frame(core);
        } else {
            self.state = State::Hold(MOVIES[p.spec].hold_ms * 1000);
        }
    }

    pub fn update(&mut self, dt_us: u64, core: &mut Core) {
        self.now_us += dt_us;
        let mut retraces = self.retrace.advance(dt_us);
        let mut hold_budget = dt_us;
        // Bounded: each pass either consumes time or moves to a later state.
        for _ in 0..10_000 {
            match &mut self.state {
                State::Done => return,
                State::Open(n) => {
                    let n = *n;
                    self.open(core, n);
                }
                State::Frame => {
                    let Some(p) = self.cur.as_mut() else { return self.finish(core) };
                    if self.now_us.saturating_sub(p.last_us) < p.speed_ms * 1000 {
                        return;
                    }
                    // 0x113c0: the first frame re-anchors the schedule.
                    if p.decoded < 2 {
                        p.last_us = self.now_us;
                    } else {
                        p.last_us += p.speed_ms * 1000;
                    }
                    if p.late {
                        self.advance(core);
                        continue;
                    }
                    if p.pal_changed {
                        p.pal_changed = false;
                        if p.i == 0 {
                            self.state = State::FadeIn(Fade::new(0, 0x20, &BLACK, &p.pal));
                            continue;
                        }
                        core.video.set_palette(&p.pal);
                    }
                    Self::blit(p, core);
                    self.advance(core);
                }
                State::FadeIn(fade) => {
                    if retraces == 0 {
                        return;
                    }
                    retraces -= 1;
                    if fade.step(&mut core.video.dac) {
                        if let Some(p) = self.cur.as_ref() {
                            Self::blit(p, core);
                        }
                        self.advance(core);
                    }
                }
                State::Hold(left) => {
                    if *left > hold_budget {
                        *left -= hold_budget;
                        return;
                    }
                    hold_budget -= *left;
                    let pal = self.cur.as_ref().map_or(BLACK, |p| p.pal);
                    self.state = State::FadeOut(Fade::new(1, 100, &pal, &BLACK));
                }
                State::FadeOut(fade) => {
                    if retraces == 0 {
                        return;
                    }
                    retraces -= 1;
                    if fade.step(&mut core.video.dac) {
                        core.video.fill_screen(0);
                        let next = self.cur.as_ref().map_or(MOVIES.len(), |p| p.spec + 1);
                        self.cur = None;
                        self.state = State::Open(next);
                    }
                }
            }
        }
    }
}
