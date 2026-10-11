//! Keyboard as the original sees it.
//!
//! In the menus `P.EXE` reads keys through the BIOS (int 16h: `bios_kbhit`
//! `0x359a3`/`0x35982`, ASCII in `0x51d2f0`, scancode in `0x51d2f4`); during
//! play it hooks int 9 and keeps a key table (`g_keys`, `0x51d1f0`). Both
//! views are rebuilt here from the frontend's raw key events
//! (`App::key_event`). Buttons that arrive only through [`Input`] (gamepad,
//! tests that script `Input`) are turned into the matching key.

use std::collections::VecDeque;

use pd_engine::Input;

/// Key-table indices (set-1 make code, `+ 0x80` for `E0` keys).
pub mod sc {
    pub const ESC: u8 = 0x01;
    pub const BACKSPACE: u8 = 0x0e;
    pub const TAB: u8 = 0x0f;
    pub const ENTER: u8 = 0x1c;
    pub const LCTRL: u8 = 0x1d;
    pub const LSHIFT: u8 = 0x2a;
    pub const RSHIFT: u8 = 0x36;
    pub const SPACE: u8 = 0x39;
    pub const CAPS: u8 = 0x3a;
    pub const UP: u8 = 0xc8;
    pub const DOWN: u8 = 0xd0;
    pub const LEFT: u8 = 0xcb;
    pub const RIGHT: u8 = 0xcd;
}

/// A key as int 16h AH=0 returns it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BiosKey {
    pub ascii: u8,
    pub scan: u8,
}

/// BIOS type-ahead buffer size (15 keys fit in the 16-word ring).
const BUFFER: usize = 15;
/// Typematic defaults of an AT keyboard: 500 ms delay, 10.9 characters per
/// second, in 70 Hz ticks. (The original relies on the BIOS for this.)
const REPEAT_DELAY: u32 = 35;
const REPEAT_PERIOD_NUM: u32 = 700; // ticks * 109 per repeat
const REPEAT_PERIOD_DEN: u32 = 109;

const UNSHIFTED: &[u8; 0x3a] = b"\0\x1b1234567890-=\x08\tqwertyuiop[]\r\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
const SHIFTED: &[u8; 0x3a] = b"\0\x1b!@#$%^&*()_+\x08\0QWERTYUIOP{}\r\0ASDFGHJKL:\"~\0|ZXCVBNM<>?\0*\0 ";

pub struct Keyboard {
    down: [bool; 256],
    buf: VecDeque<BiosKey>,
    caps: bool,
    /// Key that auto-repeats and the ticks (x109) until its next repeat.
    repeat: Option<(u8, u32)>,
    /// `false` while the game's own int 9 handler is installed (`kbd_install`
    /// `0x31b90`): the BIOS buffer then receives nothing.
    bios: bool,
    prev: Input,
    /// Keys synthesised from `Input` that are currently held.
    synth: [bool; 256],
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

/// `Input` buttons and the key each stands for when no real key explains it.
fn input_keys(i: &Input) -> [(bool, u8); 9] {
    [
        (i.up, sc::UP),
        (i.down, sc::DOWN),
        (i.left, sc::LEFT),
        (i.right, sc::RIGHT),
        (i.item, sc::ENTER),
        (i.cancel, sc::ESC),
        (i.secondary, sc::SPACE),
        (i.next_item, sc::BACKSPACE),
        (i.inventory, sc::TAB),
    ]
}

/// Keys the frontends bind to the same button as `key` (their binding table,
/// `pd_engine::input::Button::from_key_code`): WASD and the numpad for the
/// arrows, keypad Enter. While one of these is down the button is explained
/// by a real key and nothing is synthesised.
fn aliases(key: u8) -> &'static [u8] {
    match key {
        sc::UP => &[sc::UP, 0x11, 0x48],
        sc::DOWN => &[sc::DOWN, 0x1f, 0x50],
        sc::LEFT => &[sc::LEFT, 0x1e, 0x4b],
        sc::RIGHT => &[sc::RIGHT, 0x20, 0x4d],
        sc::ENTER => &[sc::ENTER, 0x9c],
        sc::ESC => &[sc::ESC],
        sc::SPACE => &[sc::SPACE],
        sc::BACKSPACE => &[sc::BACKSPACE],
        _ => &[sc::TAB],
    }
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            down: [false; 256],
            buf: VecDeque::new(),
            caps: false,
            repeat: None,
            bios: true,
            prev: Input::default(),
            synth: [false; 256],
        }
    }

    /// The BIOS word for a key press, `None` for keys that produce none
    /// (modifiers and the like).
    fn translate(&self, code: u8) -> Option<BiosKey> {
        let scan = code & 0x7f;
        if code >= 0x80 {
            // E0 keys: cursor block and keypad Enter / slash.
            return match scan {
                0x1c => Some(BiosKey { ascii: 0x0d, scan: 0x1c }),
                0x35 => Some(BiosKey { ascii: b'/', scan: 0x35 }),
                0x47..=0x53 => Some(BiosKey { ascii: 0, scan }),
                _ => None,
            };
        }
        match scan {
            0x3b..=0x44 | 0x57 | 0x58 => Some(BiosKey { ascii: 0, scan }),
            // Keypad without NumLock: cursor keys; the centre key gives nothing.
            0x47..=0x49 | 0x4b | 0x4d | 0x4f..=0x53 => Some(BiosKey { ascii: 0, scan }),
            0x4a => Some(BiosKey { ascii: b'-', scan }),
            0x4e => Some(BiosKey { ascii: b'+', scan }),
            0..=0x39 => {
                let shift = self.down[sc::LSHIFT as usize] || self.down[sc::RSHIFT as usize];
                let plain = UNSHIFTED[scan as usize];
                let upper = if plain.is_ascii_lowercase() { shift != self.caps } else { shift };
                let ascii = if upper { SHIFTED[scan as usize] } else { plain };
                (ascii != 0 || scan == 0x0f).then_some(BiosKey { ascii, scan })
            }
            _ => None,
        }
    }

    fn press(&mut self, code: u8) {
        if code == sc::CAPS {
            self.caps = !self.caps;
        }
        if !self.bios {
            return;
        }
        if let Some(k) = self.translate(code) {
            if self.buf.len() < BUFFER {
                self.buf.push_back(k);
            }
            self.repeat = Some((code, (REPEAT_DELAY + 1) * REPEAT_PERIOD_DEN));
        }
    }

    /// Raw key event from the frontend (`App::key_event`).
    pub fn key_event(&mut self, code: u8, pressed: bool) {
        let was = self.down[code as usize];
        self.down[code as usize] = pressed;
        if pressed && !was {
            self.press(code);
        } else if !pressed && self.repeat.is_some_and(|(c, _)| c == code) {
            self.repeat = None;
        }
    }

    /// Once per 70 Hz tick: typematic repeat, and keys for `Input` buttons
    /// that no real key accounts for.
    pub fn tick(&mut self, input: &Input) {
        for ((now, key), (before, _)) in input_keys(input).into_iter().zip(input_keys(&self.prev)) {
            let explained = aliases(key).iter().any(|&a| self.down[a as usize] && !self.synth[a as usize]);
            if now && !before && !explained && !self.down[key as usize] {
                self.synth[key as usize] = true;
                self.key_event(key, true);
            } else if !now && self.synth[key as usize] {
                self.synth[key as usize] = false;
                self.key_event(key, false);
            }
        }
        self.prev = *input;

        if let Some((code, left)) = self.repeat.as_mut() {
            if !self.bios {
                self.repeat = None;
            } else if *left >= REPEAT_PERIOD_DEN {
                *left -= REPEAT_PERIOD_DEN;
            }
        }
        if let Some((code, left)) = self.repeat.as_mut() {
            if *left < REPEAT_PERIOD_DEN {
                *left += REPEAT_PERIOD_NUM;
                let code = *code;
                if let Some(k) = self.translate(code) {
                    if self.buf.len() < BUFFER {
                        self.buf.push_back(k);
                    }
                }
            }
        }
    }

    /// `bios_kbhit` (`0x359a3`).
    pub fn kbhit(&self) -> bool {
        !self.buf.is_empty()
    }

    /// `bios_getkey` without blocking: the next key, if any.
    pub fn getkey(&mut self) -> Option<BiosKey> {
        self.buf.pop_front()
    }

    /// `kbd_flush` (`0x359be`).
    pub fn flush(&mut self) {
        self.buf.clear();
    }

    /// `kbd_install` / `kbd_remove`: switch between the game's key table
    /// (no BIOS keys) and BIOS input.
    pub fn set_bios(&mut self, on: bool) {
        self.bios = on;
        if !on {
            self.repeat = None;
        }
    }

    /// `g_keys[code] != 0`.
    pub fn is_down(&self, code: u8) -> bool {
        self.down[code as usize]
    }

    /// Number of keys down and whether exactly the three given are
    /// (the cheat test of `input_poll`, `0x26ec0`).
    pub fn exactly(&self, codes: [u8; 3]) -> bool {
        self.down.iter().filter(|&&d| d).count() == 3 && codes.iter().all(|&c| self.down[c as usize])
    }

    pub fn any_down(&self) -> bool {
        self.down.iter().any(|&d| d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bios_translation() {
        let mut k = Keyboard::new();
        k.key_event(0x14, true); // t
        k.key_event(0x14, false);
        k.key_event(sc::LSHIFT, true);
        k.key_event(0x14, true);
        k.key_event(0x14, false);
        k.key_event(sc::LSHIFT, false);
        k.key_event(sc::UP, true);
        k.key_event(sc::UP, false);
        k.key_event(sc::ENTER, true);
        k.key_event(sc::ENTER, false);
        assert_eq!(k.getkey(), Some(BiosKey { ascii: b't', scan: 0x14 }));
        assert_eq!(k.getkey(), Some(BiosKey { ascii: b'T', scan: 0x14 }));
        assert_eq!(k.getkey(), Some(BiosKey { ascii: 0, scan: 0x48 }));
        assert_eq!(k.getkey(), Some(BiosKey { ascii: 0x0d, scan: 0x1c }));
        assert_eq!(k.getkey(), None);
    }

    #[test]
    fn typematic_and_synthesis() {
        let mut k = Keyboard::new();
        let held = Input { down: true, ..Input::default() };
        k.tick(&held);
        assert_eq!(k.getkey(), Some(BiosKey { ascii: 0, scan: 0x50 }));
        for _ in 0..34 {
            k.tick(&held);
        }
        assert!(!k.kbhit());
        for _ in 0..3 {
            k.tick(&held);
        }
        assert_eq!(k.getkey(), Some(BiosKey { ascii: 0, scan: 0x50 }));
        k.flush();
        k.tick(&Input::default());
        for _ in 0..100 {
            k.tick(&Input::default());
        }
        assert!(!k.kbhit());

        // W held (bound to Up by the frontends) is a letter, not a cursor key.
        k.key_event(0x11, true);
        k.tick(&Input { up: true, ..Input::default() });
        assert_eq!(k.getkey(), Some(BiosKey { ascii: b'w', scan: 0x11 }));
        assert_eq!(k.getkey(), None);
    }
}
