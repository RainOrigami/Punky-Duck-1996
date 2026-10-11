//! Frontend-side helper that merges keyboard, gamepad and mouse state into an
//! [`Input`] snapshot. Device handling itself stays in the frontends.

use crate::{Input, SCREEN_H, SCREEN_W};

/// The digital buttons of [`Input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Action,
    Secondary,
    Inventory,
    Cancel,
    Item,
    NextItem,
    View,
    Interlace,
    Misc,
    Key1,
    Key2,
    Pause,
}

/// Number of [`Button`]s.
pub const BUTTONS: usize = 16;

impl Button {
    /// In discriminant order; the web frontend's gamepad bit `i` is `ALL[i]`.
    pub const ALL: [Button; BUTTONS] = [
        Button::Up,
        Button::Down,
        Button::Left,
        Button::Right,
        Button::Action,
        Button::Secondary,
        Button::Inventory,
        Button::Cancel,
        Button::Item,
        Button::NextItem,
        Button::View,
        Button::Interlace,
        Button::Misc,
        Button::Key1,
        Button::Key2,
        Button::Pause,
    ];

    /// Keyboard binding shared by both frontends. `code` is a W3C
    /// `KeyboardEvent.code` value (the browser reports these directly; winit's
    /// `KeyCode` variants carry the same names).
    ///
    /// These are the original's keys (`docs/re/engine/input.md`); WASD and
    /// the numpad are added as alternative direction keys.
    pub fn from_key_code(code: &str) -> Option<Button> {
        Some(match code {
            "ArrowUp" | "KeyW" | "Numpad8" => Button::Up,
            "ArrowDown" | "KeyS" | "Numpad2" => Button::Down,
            "ArrowLeft" | "KeyA" | "Numpad4" => Button::Left,
            "ArrowRight" | "KeyD" | "Numpad6" => Button::Right,
            "ControlLeft" | "ControlRight" => Button::Action,
            "Space" => Button::Secondary,
            "Tab" => Button::Inventory,
            "Escape" => Button::Cancel,
            "Enter" | "NumpadEnter" => Button::Item,
            "Backspace" => Button::NextItem,
            "F1" => Button::View,
            "F2" => Button::Interlace,
            "Home" | "PageUp" | "End" | "PageDown" | "Insert" | "Delete" => Button::Misc,
            "Digit1" => Button::Key1,
            "Digit2" => Button::Key2,
            "KeyP" | "Pause" => Button::Pause,
            _ => return None,
        })
    }
}

/// The original's key-table index for a W3C `KeyboardEvent.code` (winit's
/// `KeyCode` names are the same): PC set-1 make code, `+ 0x80` for
/// `E0`-prefixed keys. This is what `App::key_event` takes. Covers the keys
/// of a PC/AT keyboard that the game can see.
pub fn scancode_from_key_code(code: &str) -> Option<u8> {
    const LETTERS: [u8; 26] = [
        0x1e, 0x30, 0x2e, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19, 0x10, 0x13,
        0x1f, 0x14, 0x16, 0x2f, 0x11, 0x2d, 0x15, 0x2c,
    ];
    if let Some(l) = code.strip_prefix("Key") {
        let b = l.as_bytes();
        if b.len() == 1 && b[0].is_ascii_uppercase() {
            return Some(LETTERS[(b[0] - b'A') as usize]);
        }
        return None;
    }
    if let Some(d) = code.strip_prefix("Digit") {
        let b = d.as_bytes();
        if b.len() == 1 && b[0].is_ascii_digit() {
            return Some(if b[0] == b'0' { 0x0b } else { 0x02 + (b[0] - b'1') });
        }
        return None;
    }
    Some(match code {
        "Escape" => 0x01,
        "Minus" => 0x0c,
        "Equal" => 0x0d,
        "Backspace" => 0x0e,
        "Tab" => 0x0f,
        "BracketLeft" => 0x1a,
        "BracketRight" => 0x1b,
        "Enter" => 0x1c,
        "ControlLeft" => 0x1d,
        "Semicolon" => 0x27,
        "Quote" => 0x28,
        "Backquote" => 0x29,
        "ShiftLeft" => 0x2a,
        "Backslash" => 0x2b,
        "Comma" => 0x33,
        "Period" => 0x34,
        "Slash" => 0x35,
        "ShiftRight" => 0x36,
        "NumpadMultiply" => 0x37,
        "AltLeft" => 0x38,
        "Space" => 0x39,
        "CapsLock" => 0x3a,
        "F1" => 0x3b,
        "F2" => 0x3c,
        "F3" => 0x3d,
        "F4" => 0x3e,
        "F5" => 0x3f,
        "F6" => 0x40,
        "F7" => 0x41,
        "F8" => 0x42,
        "F9" => 0x43,
        "F10" => 0x44,
        "NumLock" => 0x45,
        "ScrollLock" => 0x46,
        "Numpad7" => 0x47,
        "Numpad8" => 0x48,
        "Numpad9" => 0x49,
        "NumpadSubtract" => 0x4a,
        "Numpad4" => 0x4b,
        "Numpad5" => 0x4c,
        "Numpad6" => 0x4d,
        "NumpadAdd" => 0x4e,
        "Numpad1" => 0x4f,
        "Numpad2" => 0x50,
        "Numpad3" => 0x51,
        "Numpad0" => 0x52,
        "NumpadDecimal" => 0x53,
        "F11" => 0x57,
        "F12" => 0x58,
        "NumpadEnter" => 0x9c,
        "ControlRight" => 0x9d,
        "NumpadDivide" => 0xb5,
        "AltRight" => 0xb8,
        "Home" => 0xc7,
        "ArrowUp" => 0xc8,
        "PageUp" => 0xc9,
        "ArrowLeft" => 0xcb,
        "ArrowRight" => 0xcd,
        "End" => 0xcf,
        "ArrowDown" => 0xd0,
        "PageDown" => 0xd1,
        "Insert" => 0xd2,
        "Delete" => 0xd3,
        _ => return None,
    })
}

/// Which device a button state comes from; sources are OR-ed together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Keyboard = 0,
    Gamepad = 1,
}

/// Accumulated device state.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    held: [[bool; BUTTONS]; 2],
    mouse_x: i32,
    mouse_y: i32,
    mouse_left: bool,
    mouse_right: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, source: Source, button: Button, pressed: bool) {
        self.held[source as usize][button as usize] = pressed;
    }

    /// Release everything from `source` (focus loss, gamepad unplugged).
    pub fn clear(&mut self, source: Source) {
        self.held[source as usize] = [false; BUTTONS];
    }

    /// OR an analogue stick direction into the gamepad state. Frontends that
    /// poll call `clear(Source::Gamepad)`, then `set` per button, then this.
    /// `x`/`y` are in `-1.0..=1.0` with `y` positive = down.
    pub fn set_gamepad_axes(&mut self, x: f32, y: f32) {
        const DEADZONE: f32 = 0.5;
        let pad = &mut self.held[Source::Gamepad as usize];
        pad[Button::Left as usize] |= x < -DEADZONE;
        pad[Button::Right as usize] |= x > DEADZONE;
        pad[Button::Up as usize] |= y < -DEADZONE;
        pad[Button::Down as usize] |= y > DEADZONE;
    }

    /// Mouse position in framebuffer pixels (clamped to the screen).
    pub fn set_mouse_pos(&mut self, x: i32, y: i32) {
        self.mouse_x = x.clamp(0, SCREEN_W as i32 - 1);
        self.mouse_y = y.clamp(0, SCREEN_H as i32 - 1);
    }

    pub fn set_mouse_buttons(&mut self, left: Option<bool>, right: Option<bool>) {
        if let Some(l) = left {
            self.mouse_left = l;
        }
        if let Some(r) = right {
            self.mouse_right = r;
        }
    }

    pub fn is_held(&self, button: Button) -> bool {
        self.held.iter().any(|s| s[button as usize])
    }

    /// Snapshot for `App::update`.
    pub fn input(&self) -> Input {
        Input {
            up: self.is_held(Button::Up),
            down: self.is_held(Button::Down),
            left: self.is_held(Button::Left),
            right: self.is_held(Button::Right),
            action: self.is_held(Button::Action),
            secondary: self.is_held(Button::Secondary),
            inventory: self.is_held(Button::Inventory),
            cancel: self.is_held(Button::Cancel),
            mouse_x: self.mouse_x,
            mouse_y: self.mouse_y,
            mouse_left: self.mouse_left,
            mouse_right: self.mouse_right,
            item: self.is_held(Button::Item),
            next_item: self.is_held(Button::NextItem),
            view: self.is_held(Button::View),
            interlace: self.is_held(Button::Interlace),
            misc: self.is_held(Button::Misc),
            key1: self.is_held(Button::Key1),
            key2: self.is_held(Button::Key2),
            pause: self.is_held(Button::Pause),
        }
    }
}

/// Largest rectangle with the framebuffer's 4:3 shape that fits a
/// `width` x `height` surface, centred: `(x, y, w, h)`. With `integer` the
/// scale factor is rounded down to a whole number when the surface is at
/// least 640x480.
pub fn letterbox(width: u32, height: u32, integer: bool) -> (u32, u32, u32, u32) {
    let (sw, sh) = (SCREEN_W as u32, SCREEN_H as u32);
    if width == 0 || height == 0 {
        return (0, 0, 0, 0);
    }
    let (w, h) = if integer && width >= sw && height >= sh {
        let k = (width / sw).min(height / sh);
        (sw * k, sh * k)
    } else if (width as u64) * (sh as u64) <= (height as u64) * (sw as u64) {
        (width, (width as u64 * sh as u64 / sw as u64) as u32)
    } else {
        ((height as u64 * sw as u64 / sh as u64) as u32, height)
    };
    ((width - w) / 2, (height - h) / 2, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_merged() {
        let mut s = InputState::new();
        s.set(Source::Keyboard, Button::Up, true);
        s.set(Source::Gamepad, Button::Action, true);
        s.set_gamepad_axes(0.9, 0.0);
        let i = s.input();
        assert!(i.up && i.action && i.right && !i.left);
        s.clear(Source::Gamepad);
        let i = s.input();
        assert!(i.up && !i.action && !i.right);
        assert_eq!(Button::from_key_code("ArrowLeft"), Some(Button::Left));
        assert_eq!(Button::from_key_code("F11"), None);
        assert_eq!(Button::from_key_code("ControlLeft"), Some(Button::Action));
        for (i, b) in Button::ALL.into_iter().enumerate() {
            assert_eq!(b as usize, i);
        }
        s.set(Source::Keyboard, Button::Pause, true);
        assert!(s.input().pause);
        assert_eq!(scancode_from_key_code("KeyT"), Some(0x14));
        assert_eq!(scancode_from_key_code("KeyH"), Some(0x23));
        assert_eq!(scancode_from_key_code("Digit1"), Some(0x02));
        assert_eq!(scancode_from_key_code("Digit0"), Some(0x0b));
        assert_eq!(scancode_from_key_code("ArrowUp"), Some(0xc8));
        assert_eq!(scancode_from_key_code("Enter"), Some(0x1c));
        assert_eq!(scancode_from_key_code("Unidentified"), None);
    }

    #[test]
    fn letterboxing() {
        assert_eq!(letterbox(640, 480, false), (0, 0, 640, 480));
        assert_eq!(letterbox(2560, 1440, false), (320, 0, 1920, 1440));
        assert_eq!(letterbox(2560, 1440, true), (320, 0, 1920, 1440));
        assert_eq!(letterbox(1440, 2560, true), (80, 800, 1280, 960));
        assert_eq!(letterbox(1000, 1000, false), (0, 125, 1000, 750));
        assert_eq!(letterbox(320, 480, true), (0, 120, 320, 240));
    }
}
