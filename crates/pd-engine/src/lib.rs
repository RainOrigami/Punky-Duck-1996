//! Platform contract between the game core and its frontends (native, web).
//!
//! Everything in the core is driven from outside: the frontend owns the loop,
//! the clock, the window, the audio device and the asset bytes. Nothing in
//! `pd-formats`, `pd-engine` or `pd-game` may touch `std::fs`, `std::time`,
//! threads, or block waiting for input.

pub mod assets;
pub mod audio;
pub mod demo;
pub mod input;
pub mod movie;
pub mod runner;

pub const SCREEN_W: usize = 640;
pub const SCREEN_H: usize = 480;

/// Mixer output format handed to the frontend.
pub const SAMPLE_RATE: u32 = 44_100;
pub const CHANNELS: usize = 2;

/// 8-bit indexed framebuffer plus its 256-entry RGB palette (8 bits per channel).
pub struct Framebuffer {
    pub pixels: Box<[u8; SCREEN_W * SCREEN_H]>,
    pub palette: [[u8; 3]; 256],
}

impl Framebuffer {
    pub fn new() -> Self {
        let pixels = vec![0u8; SCREEN_W * SCREEN_H]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        Self { pixels, palette: [[0; 3]; 256] }
    }

    /// Expand to RGBA8 for presentation. `out` must hold `SCREEN_W * SCREEN_H * 4` bytes.
    pub fn to_rgba(&self, out: &mut [u8]) {
        for (px, dst) in self.pixels.iter().zip(out.chunks_exact_mut(4)) {
            let [r, g, b] = self.palette[*px as usize];
            dst.copy_from_slice(&[r, g, b, 0xff]);
        }
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only access to the original game files by name (case-insensitive),
/// e.g. `"BANQUE.BNK"`. Native reads the disc image or an extracted directory;
/// web serves bytes from a user-supplied image held in memory.
pub trait AssetSource {
    fn load(&self, name: &str) -> Option<Vec<u8>>;
}

/// Persistent storage for settings and saves (file on native, localStorage on web).
pub trait Storage {
    fn read(&self, key: &str) -> Option<Vec<u8>>;
    fn write(&mut self, key: &str, data: &[u8]);
}

/// Input snapshot for one tick. Frontends map keyboard/gamepad/mouse onto this.
///
/// All fields are levels (held = true); edge detection is the app's job, as
/// in the original. The comments name the key of the original's input record
/// (`docs/re/engine/input.md`), which is also the default keyboard binding.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// Ctrl (record byte 4, fire/action).
    pub action: bool,
    /// Space (byte 9).
    pub secondary: bool,
    /// Tab (byte 10).
    pub inventory: bool,
    /// Esc (byte 12).
    pub cancel: bool,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_left: bool,
    pub mouse_right: bool,
    /// Enter (byte 5, toggle timed item; confirms in menus).
    pub item: bool,
    /// Backspace (byte 6, next weapon/item).
    pub next_item: bool,
    /// F1 (byte 7, cycle view mode).
    pub view: bool,
    /// F2 (byte 8, toggle interlace).
    pub interlace: bool,
    /// Any of Home, PgUp, End, PgDn, Ins, Del (byte 11).
    pub misc: bool,
    /// `1` (byte 13).
    pub key1: bool,
    /// `2` (byte 14).
    pub key2: bool,
    /// P or Pause.
    pub pause: bool,
}

/// Requests from the game to the frontend, drained once per tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Play Red Book audio track `track` (2..=9 on the original disc).
    PlayCdTrack { track: u8, looping: bool },
    StopCd,
    Quit,
    /// CD music volume as the original hands it to the drive: `GAME.STP`
    /// CD volume x 16, so `0..=240` (`cd_set_volume`, `0x1e950`). 255 = full.
    SetCdVolume { volume: u8 },
}

/// A running game or sub-program (intro player, main game, end sequence).
///
/// Blocking waits from the DOS original (wait-for-key, fades, movie playback)
/// must be ported as state advanced by `update`, never as loops.
pub trait App {
    /// Advance by `dt_us` microseconds of game time.
    fn update(&mut self, dt_us: u64, input: &Input, commands: &mut Vec<Command>);
    fn render(&mut self, fb: &mut Framebuffer);
    /// Fill `out` with interleaved stereo i16 at [`SAMPLE_RATE`] (sound effects only;
    /// CD music is mixed in by the frontend).
    fn mix_audio(&mut self, out: &mut [i16]);

    /// A key went down or up. `scancode` is the index the original's keyboard
    /// handler uses for its key table (`docs/re/engine/input.md`): the PC
    /// set-1 make code, plus `0x80` for `E0`-prefixed keys (arrows, right
    /// Ctrl, Home...). Frontends translate with
    /// [`input::scancode_from_key_code`] and deliver every key, including
    /// the ones that also drive [`Input`]; auto-repeat is not delivered.
    /// Apps that only need [`Input`] ignore this.
    fn key_event(&mut self, _scancode: u8, _pressed: bool) {}

    /// Called by the runner before every `update` with the state of the CD
    /// player (`cd_is_busy`, `0x1e880`): the original restarts its music by
    /// polling the drive.
    fn cd_status(&mut self, _playing: bool) {}

    /// Relative mouse movement in device pixels since the last call. Delivered
    /// while the pointer is locked (see [`App::wants_pointer_lock`]).
    fn mouse_motion(&mut self, _dx: f32, _dy: f32) {}

    /// `true` while the app wants the pointer hidden and locked to the window
    /// for relative mouse input. Frontends poll this every frame and lock or
    /// release accordingly.
    fn wants_pointer_lock(&self) -> bool {
        false
    }
}
