//! State shared by every part of the program flow, and the queue of
//! "blocking" operations.
//!
//! The DOS program is straight-line code full of waits (fade, wait for a key,
//! play a movie, delay). Here each wait is an [`Op`] in a queue that
//! [`Core::run_ops`] advances once per 70 Hz tick; the flow code queues ops
//! and continues when the queue is empty.

use std::collections::VecDeque;

use pd_engine::{AssetSource, Command, Input, Storage};
use pd_formats::bnk::Bnk;
use pd_formats::flc::Flc;
use pd_formats::ron;

use super::gfx::{self, Fade, FontState, Pal, Sheet, Video};
use super::keys::{BiosKey, Keyboard};
use super::sound::Sound;
use super::store::Settings;

/// A decoded `.RON` picture with the palette as `load_ron` (`0x2b5f0`)
/// leaves it: components shifted down to 6 bits.
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u8>,
    pub pal: Pal,
}

/// `flc_play(path, loops)` (`0x2bc70`): `loops x frames` frame reads, each
/// after `n` vertical retraces, drawn straight to the screen; the player
/// wraps around through the ring frame, so every pass shows the same frames.
pub struct FlcPlay {
    flc: Flc<Vec<u8>>,
    /// Retraces per frame: `trunc(header speed / 14.28)` (`0x2bc94`).
    n: u32,
    waited: u32,
    reads_left: u32,
}

impl FlcPlay {
    pub fn new(data: Vec<u8>, loops: u32) -> Option<Self> {
        let flc = Flc::new(data).ok()?;
        let h = *flc.header();
        // fild word [speed] / fdiv 14.28 / __CHP (truncate).
        let n = ((h.speed_ms & 0xffff) as f64 / 14.28) as u32;
        Some(Self { flc, n, waited: 0, reads_left: loops * h.frames as u32 })
    }

    /// Retraces per frame.
    pub fn frame_ticks(&self) -> u32 {
        self.n
    }

    /// One retrace. Returns `true` when playback is over.
    fn tick(&mut self, video: &mut Video) -> bool {
        if self.reads_left == 0 {
            return true;
        }
        self.waited += 1;
        if self.waited < self.n {
            return false;
        }
        self.waited = 0;
        let info = match self.flc.decode_next() {
            Ok(Some(info)) => Some(info),
            // End of a pass: the original decodes the ring frame, which
            // turns the last picture back into the first, and continues
            // with the second frame. Restarting gives the same pictures.
            Ok(None) => {
                self.flc.rewind();
                self.flc.decode_next().ok().flatten()
            }
            Err(_) => None,
        };
        let Some(info) = info else {
            self.reads_left = 0;
            return true;
        };
        if info.palette_changed {
            for (dst, src) in video.dac.iter_mut().zip(self.flc.palette()) {
                *dst = src.map(|c| c >> 2);
            }
        }
        let (w, h) = (self.flc.width().min(gfx::W), self.flc.height().min(gfx::H));
        for (y, row) in self.flc.pixels().chunks_exact(self.flc.width()).take(h).enumerate() {
            video.screen.pixels[y * gfx::W..y * gfx::W + w].copy_from_slice(&row[..w]);
        }
        self.reads_left -= 1;
        self.reads_left == 0
    }
}

pub enum Op {
    /// `palette_fade`: one step per retrace.
    Fade(Fade),
    /// Busy wait of that many ticks (`delay()`, loading pauses).
    Delay(u32),
    /// Blocking `bios_getkey` (`0x359a3`); the key lands in [`Core::last_key`].
    WaitKey,
    /// The picture wait: `do {} while (!input_any()); do {} while (input_any());`
    /// (`0x2a2f0`). `released == false` is the first loop.
    WaitInput { released: bool },
    Movie(Box<FlcPlay>),
    /// Poll until the condition holds (one test per tick).
    Until(Box<dyn Fn(&Core) -> bool>),
    /// Runs at once, between waits.
    Do(Box<dyn FnOnce(&mut Core)>),
}

pub struct Core {
    pub assets: Box<dyn AssetSource>,
    pub storage: Box<dyn Storage>,
    pub video: Video,
    pub kbd: Keyboard,
    pub sound: Sound,
    pub settings: Settings,
    /// Commands for the frontend, drained by `App::update`.
    pub commands: Vec<Command>,
    pub ops: VecDeque<Op>,
    /// Input snapshot of this tick and the previous one.
    pub input: Input,
    pub prev_input: Input,
    /// `cd_is_busy` as last reported by the frontend.
    pub cd_playing: bool,
    /// Relative mouse movement not yet consumed (modern controls).
    pub mouse_dx: f32,
    /// Result of the last [`Op::WaitKey`].
    pub last_key: BiosKey,

    banque: Option<Vec<u8>>,
    picture: Option<Vec<u8>>,

    /// `COOLFNT1..3.SPT` (`0x555d0`, `0x72a90`, `0x79fc0`).
    pub coolfnt1: Sheet,
    pub coolfnt2: Sheet,
    pub coolfnt3: Sheet,
    /// State of the last `font_set` (`0x32bb0`).
    pub font: FontState,

    /// Scratch for modal screens: palette of the picture on screen and a
    /// clean copy of the back buffer.
    pub stash_pal: Pal,
    pub stash_back: Option<Vec<u8>>,

    /// Flow events in order (`"cd 4"`, `"menu"`, `"island 1"`...), for
    /// `--trace` and tests.
    pub trace: Vec<String>,
    /// Problems that the original would have crashed or hung on (missing files).
    pub errors: Vec<String>,
    /// 70 Hz ticks since start.
    pub ticks: u64,
}

impl Core {
    pub fn new(assets: Box<dyn AssetSource>, storage: Box<dyn Storage>) -> Self {
        let sound = Sound::load(assets.as_ref());
        let banque = assets.load("BANQUE.BNK");
        let picture = assets.load("PICTURE.BNK");
        let mut core = Self {
            assets,
            storage,
            video: Video::new(),
            kbd: Keyboard::new(),
            sound,
            settings: Settings::builtin(),
            commands: Vec::new(),
            ops: VecDeque::new(),
            input: Input::default(),
            prev_input: Input::default(),
            cd_playing: false,
            mouse_dx: 0.0,
            last_key: BiosKey::default(),
            banque,
            picture,
            coolfnt1: Sheet::empty(),
            coolfnt2: Sheet::empty(),
            coolfnt3: Sheet::empty(),
            font: FontState { first_char: 0x20, limit: 0, spacing: 0 },
            stash_pal: gfx::BLACK,
            stash_back: None,
            trace: Vec::new(),
            errors: Vec::new(),
            ticks: 0,
        };
        if core.banque.is_none() {
            core.errors.push("BANQUE.BNK: not found".into());
        }
        if core.picture.is_none() {
            core.errors.push("PICTURE.BNK: not found".into());
        }
        core
    }

    pub fn note(&mut self, what: impl Into<String>) {
        if self.trace.len() < 10_000 {
            self.trace.push(what.into());
        }
    }

    /// Raw bytes of an entry of `BANQUE.BNK` (`load_banque`, `0x2bc20`).
    pub fn banque(&self, name: &str) -> Option<&[u8]> {
        Bnk::parse(self.banque.as_deref()?).ok()?.get(name)
    }

    /// `load_banque` + parse as a sprite sheet. Missing sheets are empty.
    pub fn load_sheet(&mut self, name: &str) -> Sheet {
        match self.banque(name).and_then(Sheet::parse) {
            Some(s) => s,
            None => {
                self.errors.push(format!("{name}: not in BANQUE.BNK"));
                Sheet::empty()
            }
        }
    }

    /// `load_ron` (`0x2b5f0`) from `PICTURE.BNK`; palette `>> 2`.
    pub fn load_picture(&mut self, name: &str) -> Option<Picture> {
        let decoded = self
            .picture
            .as_deref()
            .and_then(|d| Bnk::parse(d).ok())
            .and_then(|b| b.get(name))
            .and_then(|d| ron::decode(d).ok());
        match decoded {
            Some(r) => Some(Picture {
                w: r.width as usize,
                h: r.height as usize,
                pal: r.palette_vga6(),
                pixels: r.pixels,
            }),
            None => {
                self.errors.push(format!("{name}: not in PICTURE.BNK"));
                None
            }
        }
    }

    /// Undecoded entry of `PICTURE.BNK`.
    pub fn load_picture_raw(&self, name: &str) -> Option<Vec<u8>> {
        Bnk::parse(self.picture.as_deref()?).ok()?.get(name).map(<[u8]>::to_vec)
    }

    /// `cd_stop` (`0x1e860`).
    pub fn cd_stop(&mut self) {
        self.commands.push(Command::StopCd);
        self.cd_playing = false;
        self.note("cd stop");
    }

    /// `cd_play_track` (`0x1e740`): plays the track once; music never loops
    /// by itself.
    pub fn cd_play(&mut self, track: u8) {
        self.commands.push(Command::PlayCdTrack { track, looping: false });
        self.cd_playing = true;
        self.note(format!("cd {track}"));
    }

    /// `cd_set_volume(GAME.STP cd volume << 4)` (`0x1e950`).
    pub fn cd_volume(&mut self) {
        let v = (self.settings.stp.cd_volume << 4).min(255) as u8;
        self.commands.push(Command::SetCdVolume { volume: v });
    }

    /// Apply the sound switches as the menu does (`0x25b3a`, `0x26b6c`).
    pub fn apply_sound(&mut self) {
        let stp = self.settings.stp;
        self.sound.configure(stp.sound_on != 0, stp.sfx_volume);
    }

    // ---- queueing helpers -------------------------------------------------

    pub fn fade(&mut self, steps: i32, from: &Pal, to: &Pal) {
        self.ops.push_back(Op::Fade(Fade::new(0, steps, from, to)));
    }

    pub fn fade_from(&mut self, first: usize, steps: i32, from: &Pal, to: &Pal) {
        self.ops.push_back(Op::Fade(Fade::new(first, steps, from, to)));
    }

    pub fn then(&mut self, f: impl FnOnce(&mut Core) + 'static) {
        self.ops.push_back(Op::Do(Box::new(f)));
    }

    pub fn delay(&mut self, ticks: u32) {
        self.ops.push_back(Op::Delay(ticks));
    }

    pub fn wait_key(&mut self) {
        self.ops.push_back(Op::WaitKey);
    }

    pub fn wait_input(&mut self) {
        self.ops.push_back(Op::WaitInput { released: false });
    }

    /// `flc_play(name, loops)` followed by its screen clear. A missing file
    /// plays nothing (the original would crash).
    pub fn play_flc(&mut self, name: &'static str, loops: u32) {
        self.then(move |core| {
            core.note(format!("movie {name}"));
            match core.assets.load(name).and_then(|d| FlcPlay::new(d, loops)) {
                Some(p) => core.ops.push_front(Op::Movie(Box::new(p))),
                None => core.errors.push(format!("{name}: cannot play")),
            }
        });
        self.then(|core| core.video.fill_screen(0));
    }

    pub fn busy(&self) -> bool {
        !self.ops.is_empty()
    }

    /// `input_any` (`0x2a2f0`): any byte of the input record set. Level keys
    /// count while held, edge keys on the tick they go down.
    pub fn input_any(&self) -> bool {
        let (i, p) = (&self.input, &self.prev_input);
        i.up || i.down
            || i.left
            || i.right
            || i.action
            || i.secondary
            || (i.item && !p.item)
            || (i.next_item && !p.next_item)
            || (i.view && !p.view)
            || (i.interlace && !p.interlace)
            || (i.inventory && !p.inventory)
            || (i.misc && !p.misc)
            || (i.cancel && !p.cancel)
            || (i.key1 && !p.key1)
            || (i.key2 && !p.key2)
    }

    /// Advance the queue by one tick. Returns `true` while something is
    /// still pending (the flow code must not continue).
    pub fn run_ops(&mut self) -> bool {
        // A fade step waits for the vertical retrace: at most one per tick,
        // and nothing queued after a fade starts before its last step is out.
        let mut retrace_used = false;
        loop {
            let Some(op) = self.ops.front_mut() else { return false };
            match op {
                Op::Do(_) => {
                    if let Some(Op::Do(f)) = self.ops.pop_front() {
                        f(self);
                    }
                }
                Op::Fade(f) => {
                    if retrace_used {
                        return true;
                    }
                    retrace_used = true;
                    if f.step(&mut self.video.dac) {
                        self.ops.pop_front();
                    } else {
                        return true;
                    }
                }
                Op::Delay(n) => {
                    if *n == 0 {
                        self.ops.pop_front();
                    } else {
                        *n -= 1;
                        return true;
                    }
                }
                Op::WaitKey => match self.kbd.getkey() {
                    Some(k) => {
                        self.last_key = k;
                        self.ops.pop_front();
                    }
                    None => return true,
                },
                Op::WaitInput { released } => {
                    let any = {
                        let released = *released;
                        let any = self.input_any();
                        if !released && any {
                            if let Some(Op::WaitInput { released }) = self.ops.front_mut() {
                                *released = true;
                            }
                            return true;
                        }
                        if !released {
                            return true;
                        }
                        any
                    };
                    if any {
                        return true;
                    }
                    self.ops.pop_front();
                }
                Op::Until(_) => {
                    let Some(Op::Until(f)) = self.ops.pop_front() else { continue };
                    if !f(self) {
                        self.ops.push_front(Op::Until(f));
                        return true;
                    }
                }
                Op::Movie(m) => {
                    if retrace_used {
                        return true;
                    }
                    retrace_used = true;
                    if m.tick(&mut self.video) {
                        self.ops.pop_front();
                    } else {
                        return true;
                    }
                }
            }
        }
    }
}
