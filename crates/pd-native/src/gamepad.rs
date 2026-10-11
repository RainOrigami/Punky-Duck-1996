//! Gamepad polling through gilrs (evdev on Linux). Optional: build with
//! `--no-default-features` to leave it out.

use pd_engine::input::{Button, InputState, Source};

#[cfg(feature = "gamepad")]
pub struct Gamepads {
    gilrs: Option<gilrs::Gilrs>,
}

#[cfg(feature = "gamepad")]
impl Gamepads {
    pub fn new() -> Self {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("pd-native: gamepad support unavailable: {e}");
                None
            }
        };
        Self { gilrs }
    }

    /// Replace the gamepad part of `state` with the union of all connected pads.
    pub fn poll(&mut self, state: &mut InputState) {
        use gilrs::{Axis, Button as B};
        let Some(gilrs) = self.gilrs.as_mut() else { return };
        // Draining the event queue is what updates the cached pad state.
        while gilrs.next_event().is_some() {}
        state.clear(Source::Gamepad);
        const MAP: [(B, Button); 12] = [
            (B::DPadUp, Button::Up),
            (B::DPadDown, Button::Down),
            (B::DPadLeft, Button::Left),
            (B::DPadRight, Button::Right),
            (B::South, Button::Action),
            (B::East, Button::Secondary),
            (B::West, Button::Item),
            (B::North, Button::NextItem),
            (B::LeftTrigger, Button::View),
            (B::RightTrigger, Button::Inventory),
            (B::Start, Button::Cancel),
            (B::Select, Button::Pause),
        ];
        let mut held = [false; pd_engine::input::BUTTONS];
        let (mut x, mut y) = (0.0f32, 0.0f32);
        for (_, pad) in gilrs.gamepads() {
            for (from, to) in MAP {
                held[to as usize] |= pad.is_pressed(from);
            }
            let (px, py) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
            if px.abs() > x.abs() {
                x = px;
            }
            if py.abs() > y.abs() {
                y = py;
            }
        }
        for b in Button::ALL {
            state.set(Source::Gamepad, b, held[b as usize]);
        }
        // gilrs reports stick up as positive; InputState wants down positive.
        state.set_gamepad_axes(x, -y);
    }
}

#[cfg(not(feature = "gamepad"))]
pub struct Gamepads;

#[cfg(not(feature = "gamepad"))]
impl Gamepads {
    pub fn new() -> Self {
        Gamepads
    }

    pub fn poll(&mut self, _state: &mut InputState) {
        let _ = (Button::Up, Source::Gamepad);
    }
}
