//! Browser frontend. The page (`web/main.js`) owns everything the browser
//! owns (file pickers, canvas, requestAnimationFrame, WebAudio, key and
//! gamepad events) and drives the [`Player`] exported here, which wraps the
//! same `Runner` + app the native frontend uses.
//!
//! On targets other than wasm32 this crate is empty.
#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use pd_engine::assets::DiscAssets;
use pd_engine::demo::DemoApp;
use pd_engine::input::{Button, InputState, Source};
use pd_engine::runner::Runner;
use pd_engine::{CHANNELS, SCREEN_H, SCREEN_W, Storage};
use pd_game::app::{Game, GameOptions, Mode};
use pd_formats::disc::Disc;
use wasm_bindgen::prelude::*;

/// [`Storage`] over `window.localStorage`. Values are hex strings under
/// `punkyduck:<key>` (localStorage holds UTF-16 strings, not bytes).
pub struct WebStorage {
    storage: Option<web_sys::Storage>,
}

impl WebStorage {
    pub fn new() -> Self {
        // Unavailable in some privacy modes; then nothing persists.
        Self { storage: web_sys::window().and_then(|w| w.local_storage().ok().flatten()) }
    }

    fn key(key: &str) -> String {
        format!("punkyduck:{key}")
    }
}

impl Default for WebStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for WebStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        let hex = self.storage.as_ref()?.get_item(&Self::key(key)).ok().flatten()?;
        if hex.len() % 2 != 0 || !hex.is_ascii() {
            return None;
        }
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
    }

    fn write(&mut self, key: &str, data: &[u8]) {
        let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
        if let Some(s) = &self.storage {
            // Fails when the quota is exceeded; there is nobody to report it to here.
            let _ = s.set_item(&Self::key(key), &hex);
        }
    }
}

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct Player {
    runner: Runner,
    input: InputState,
    rgba: Vec<u8>,
    audio: Vec<i16>,
    storage: WebStorage,
    info: String,
}

#[wasm_bindgen]
impl Player {
    /// `bin` and `cue` are the contents of the user's disc image files.
    /// `mode` is `"original"`, `"enhanced"` or `""` (keep the remembered
    /// choice); `off` lists the enhancements to leave out, by name and
    /// separated by commas (see [`enhancement_list`]), and is remembered too;
    /// `demo` runs the engine demo instead of the game.
    #[wasm_bindgen(constructor)]
    pub fn new(bin: Vec<u8>, cue: &str, mode: &str, off: &str, demo: bool) -> Result<Player, JsError> {
        let disc = Rc::new(Disc::new(bin, cue).map_err(|e| JsError::new(&e.to_string()))?);
        let assets = DiscAssets::new(disc.clone());
        if !demo {
            // GAME.STP and the saves live in localStorage in the original's
            // formats; the original/enhanced choice next to them in PORT.CFG.
            let mut game_storage = WebStorage::new();
            let mode = Mode::select(Mode::parse(mode), &mut game_storage);
            if mode == Mode::Enhanced {
                pd_game::app::store::store_disabled_flags(&mut game_storage, off);
            }
            let enh = mode.stored_enhancements(&game_storage);
            let opts = GameOptions { mode, ..GameOptions::default() };
            let mut info = format!(
                "disc \"{}\": {} files, audio tracks {:?}; {} mode",
                disc.volume_id(),
                assets.names().count(),
                disc.audio_track_numbers(),
                mode.name()
            );
            let game = Game::with_enhancements(Box::new(assets), Box::new(game_storage), opts, enh);
            for e in game.errors() {
                info.push_str("\nerror: ");
                info.push_str(e);
            }
            return Ok(Player {
                runner: Runner::new(Box::new(game), Some(disc)),
                input: InputState::new(),
                rgba: vec![0; SCREEN_W * SCREEN_H * 4],
                audio: Vec::new(),
                storage: WebStorage::new(),
                info,
            });
        }
        let demo = DemoApp::new(&assets);

        // Exercise persistent storage: count how often a disc was loaded.
        let mut storage = WebStorage::new();
        let runs = storage
            .read("runs")
            .and_then(|b| <[u8; 4]>::try_from(b.as_slice()).ok())
            .map_or(0, u32::from_le_bytes)
            + 1;
        storage.write("runs", &runs.to_le_bytes());

        let mut info = format!(
            "disc \"{}\": {} files, audio tracks {:?}; run #{runs}",
            disc.volume_id(),
            assets.names().count(),
            disc.audio_track_numbers()
        );
        for e in demo.errors() {
            info.push_str("\nerror: ");
            info.push_str(e);
        }
        Ok(Player {
            runner: Runner::new(Box::new(demo), Some(disc)),
            input: InputState::new(),
            rgba: vec![0; SCREEN_W * SCREEN_H * 4],
            audio: Vec::new(),
            storage,
            info,
        })
    }

    /// One line describing the loaded disc, plus one line per load error.
    pub fn info(&self) -> String {
        self.info.clone()
    }

    /// Feed a `KeyboardEvent.code`. Returns 1 if the key is bound to a game
    /// button (the page suppresses the browser's default action), 2 if it is
    /// only a key the game can read as text (menus, name entry, cheat
    /// codes; the page suppresses it unless Ctrl/Alt/Meta is held), else 0.
    pub fn key_event(&mut self, code: &str, pressed: bool) -> u32 {
        let scancode = pd_engine::input::scancode_from_key_code(code);
        if let Some(sc) = scancode {
            self.runner.key_event(sc, pressed);
        }
        match Button::from_key_code(code) {
            Some(button) => {
                self.input.set(Source::Keyboard, button, pressed);
                1
            }
            None if scancode.is_some() => 2,
            None => 0,
        }
    }

    /// Release all keys and mouse buttons (the page lost focus).
    pub fn release_all(&mut self) {
        self.input.clear(Source::Keyboard);
        self.input.set_mouse_buttons(Some(false), Some(false));
        for sc in 0..=255u8 {
            self.runner.key_event(sc, false);
        }
    }

    /// Mouse position in framebuffer pixels (0..640, 0..480).
    pub fn set_mouse_pos(&mut self, x: i32, y: i32) {
        self.input.set_mouse_pos(x, y);
    }

    /// Relative mouse movement while the pointer is locked.
    pub fn mouse_motion(&mut self, dx: f32, dy: f32) {
        self.runner.mouse_motion(dx, dy);
    }

    /// `true` while the game wants the pointer locked to the canvas (mouse look).
    pub fn wants_pointer_lock(&self) -> bool {
        self.runner.wants_pointer_lock()
    }

    pub fn set_mouse_buttons(&mut self, left: bool, right: bool) {
        self.input.set_mouse_buttons(Some(left), Some(right));
    }

    /// Gamepad state merged over all pads. Bit `i` of `buttons` is
    /// `pd_engine::input::Button::ALL[i]` (up, down, left, right, action,
    /// secondary, inventory, cancel, item, next item, view, interlace, misc,
    /// key 1, key 2, pause); `x`/`y` is the left stick (`y` positive = down).
    pub fn set_gamepad(&mut self, buttons: u32, x: f32, y: f32) {
        self.input.clear(Source::Gamepad);
        for (i, b) in Button::ALL.into_iter().enumerate() {
            self.input.set(Source::Gamepad, b, buttons & (1 << i) != 0);
        }
        self.input.set_gamepad_axes(x, y);
    }

    /// Advance by `dt_us` microseconds and render. Read the picture through
    /// [`rgba_ptr`](Self::rgba_ptr).
    pub fn frame(&mut self, dt_us: u32) {
        self.runner.tick(dt_us as u64, &self.input.input());
        self.runner.render().to_rgba(&mut self.rgba);
    }

    /// Address of the 640x480 RGBA8 picture inside the wasm memory.
    pub fn rgba_ptr(&self) -> *const u8 {
        self.rgba.as_ptr()
    }

    /// Mix `frames` stereo frames (44.1 kHz) of sound effects and CD music;
    /// returns the address of `frames * 2` interleaved i16 samples.
    pub fn mix_audio(&mut self, frames: u32) -> *const i16 {
        self.audio.resize(frames as usize * CHANNELS, 0);
        self.runner.mix_audio(&mut self.audio);
        self.audio.as_ptr()
    }

    pub fn quit_requested(&self) -> bool {
        self.runner.quit_requested()
    }

    /// CD track being played, or 0.
    pub fn cd_track(&self) -> u32 {
        let cd = self.runner.cd();
        cd.track().filter(|_| cd.is_playing()).map_or(0, u32::from)
    }

    /// Persistent storage, as the game will use it for settings and saves.
    pub fn storage_read(&self, key: &str) -> Option<Vec<u8>> {
        self.storage.read(key)
    }

    pub fn storage_write(&mut self, key: &str, data: &[u8]) {
        self.storage.write(key, data);
    }
}

/// The remembered original/enhanced choice: `"original"` or `"enhanced"`.
#[wasm_bindgen]
pub fn stored_mode() -> String {
    Mode::load(&WebStorage::new()).name().to_string()
}

/// The enhancements one per line as `name<TAB>group<TAB>description<TAB>on`,
/// `on` being `1` unless the user switched that one off before.
#[wasm_bindgen]
pub fn enhancement_list() -> String {
    let off = pd_game::app::store::disabled_flags(&WebStorage::new());
    pd_game::settings::FLAGS
        .iter()
        .map(|(name, group, text)| format!("{name}\t{group}\t{text}\t{}", u8::from(!off.iter().any(|o| o == name))))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Framebuffer geometry and audio format for the page.
#[wasm_bindgen]
pub fn screen_width() -> u32 {
    SCREEN_W as u32
}

#[wasm_bindgen]
pub fn screen_height() -> u32 {
    SCREEN_H as u32
}

#[wasm_bindgen]
pub fn sample_rate() -> u32 {
    pd_engine::SAMPLE_RATE
}
