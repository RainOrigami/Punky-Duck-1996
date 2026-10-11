//! winit window, pixels (wgpu) presentation and the main loop.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pd_engine::input::{Button, InputState, Source};
use pd_engine::runner::Runner;
use pd_engine::{SCREEN_H, SCREEN_W};
use pd_engine::input::letterbox;
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::audio::AudioOutput;
use crate::gamepad::Gamepads;
use crate::present::Presenter;

/// The loop wakes at least this often to advance the game and feed audio,
/// independent of whether the compositor asks for frames.
const TICK: Duration = Duration::from_millis(4);

pub struct Options {
    pub fullscreen: bool,
    /// Restrict scaling to whole factors (default: largest 4:3 rectangle
    /// that fits). Both sample nearest-neighbour.
    pub integer: bool,
    pub audio: bool,
    /// Print stage changes and loop statistics.
    pub trace: bool,
    pub exit_after: Option<Duration>,
}

/// What the demo app reports back for `--trace` (stage index and name).
pub type StageProbe = Rc<Cell<(usize, &'static str)>>;

/// Keyboard bindings; same table as `Button::from_key_code` (which the web
/// frontend feeds with `KeyboardEvent.code` strings).
fn button_for(code: KeyCode) -> Option<Button> {
    Some(match code {
        KeyCode::ArrowUp | KeyCode::KeyW | KeyCode::Numpad8 => Button::Up,
        KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Numpad2 => Button::Down,
        KeyCode::ArrowLeft | KeyCode::KeyA | KeyCode::Numpad4 => Button::Left,
        KeyCode::ArrowRight | KeyCode::KeyD | KeyCode::Numpad6 => Button::Right,
        KeyCode::ControlLeft | KeyCode::ControlRight => Button::Action,
        KeyCode::Space => Button::Secondary,
        KeyCode::Tab => Button::Inventory,
        KeyCode::Escape => Button::Cancel,
        KeyCode::Enter | KeyCode::NumpadEnter => Button::Item,
        KeyCode::Backspace => Button::NextItem,
        KeyCode::F1 => Button::View,
        KeyCode::F2 => Button::Interlace,
        KeyCode::Home | KeyCode::PageUp | KeyCode::End | KeyCode::PageDown | KeyCode::Insert | KeyCode::Delete => {
            Button::Misc
        }
        KeyCode::Digit1 => Button::Key1,
        KeyCode::Digit2 => Button::Key2,
        KeyCode::KeyP | KeyCode::Pause => Button::Pause,
        _ => return None,
    })
}

struct NativeApp {
    runner: Runner,
    opts: Options,
    probe: StageProbe,
    audio: Option<AudioOutput>,
    gamepads: Gamepads,
    input: InputState,
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    presenter: Option<Presenter>,
    /// Size of the surface in physical pixels.
    surface: (u32, u32),
    integer: bool,
    alt_held: bool,
    focused: bool,
    /// Pointer hidden and locked for relative mouse input (`App::wants_pointer_lock`).
    pointer_locked: bool,
    start: Instant,
    last_tick: Instant,
    error: Option<String>,
    // --trace bookkeeping
    seen_stage: Option<usize>,
    stage_started: Instant,
    ticks: u32,
    frames: u32,
    stats_since: Instant,
}

impl NativeApp {
    fn fail(&mut self, event_loop: &ActiveEventLoop, msg: String) {
        self.error = Some(msg);
        event_loop.exit();
    }

    fn toggle_fullscreen(&self) {
        if let Some(w) = &self.window {
            w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
        }
    }

    /// Where the picture sits on the surface: `(x, y, w, h)`.
    fn picture_rect(&self) -> (u32, u32, u32, u32) {
        letterbox(self.surface.0, self.surface.1, self.integer)
    }

    fn sync_pointer_lock(&mut self) {
        let want = self.focused && self.runner.wants_pointer_lock();
        if want == self.pointer_locked {
            return;
        }
        let Some(w) = &self.window else { return };
        if want {
            // Wayland locks, X11 only confines.
            if w.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined)).is_err() {
                return;
            }
            w.set_cursor_visible(false);
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
        }
        self.pointer_locked = want;
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_pointer_lock();
        let now = Instant::now();
        let dt = now - self.last_tick;
        if dt >= Duration::from_micros(500) {
            self.last_tick = now;
            self.gamepads.poll(&mut self.input);
            self.runner.tick(dt.as_micros() as u64, &self.input.input());
            if let Some(audio) = self.audio.as_mut() {
                audio.pump(&mut self.runner);
            }
            self.ticks += 1;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            if self.opts.trace {
                self.trace(now);
            }
            if self.runner.quit_requested() || self.opts.exit_after.is_some_and(|d| now - self.start >= d) {
                event_loop.exit();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.last_tick + TICK));
    }

    fn trace(&mut self, now: Instant) {
        let (stage, name) = self.probe.get();
        if self.seen_stage != Some(stage) {
            if self.seen_stage.is_some() {
                eprintln!("[trace] previous stage ran {:.2} s", (now - self.stage_started).as_secs_f64());
            }
            let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            eprintln!(
                "[trace] t={:.2} s (unix {unix:.3}): stage {stage} ({name}), cd track {:?}",
                (now - self.start).as_secs_f64(),
                self.runner.cd().track().filter(|_| self.runner.cd().is_playing())
            );
            self.seen_stage = Some(stage);
            self.stage_started = now;
        }
        let span = now - self.stats_since;
        if span >= Duration::from_secs(2) {
            eprintln!(
                "[trace] {:.0} ticks/s, {:.1} frames/s, audio underrun samples {}",
                self.ticks as f64 / span.as_secs_f64(),
                self.frames as f64 / span.as_secs_f64(),
                self.audio.as_ref().map_or(0, AudioOutput::underrun_samples)
            );
            (self.ticks, self.frames, self.stats_since) = (0, 0, now);
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let rect = self.picture_rect();
        let (Some(pixels), Some(presenter)) = (self.pixels.as_mut(), self.presenter.as_ref()) else { return };
        self.runner.render().to_rgba(pixels.frame_mut());
        let result = pixels.render_with(|encoder, target, context| {
            presenter.render(encoder, target, context, rect);
            Ok(())
        });
        if let Err(e) = result {
            self.fail(event_loop, format!("render failed: {e}"));
            return;
        }
        self.frames += 1;
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Punky Duck")
            .with_inner_size(LogicalSize::new(SCREEN_W as f64 * 2.0, SCREEN_H as f64 * 2.0))
            .with_min_inner_size(LogicalSize::new(SCREEN_W as f64 / 2.0, SCREEN_H as f64 / 2.0))
            .with_fullscreen(self.opts.fullscreen.then_some(Fullscreen::Borderless(None)));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => return self.fail(event_loop, format!("cannot create window: {e}")),
        };
        let size = window.inner_size();
        self.surface = (size.width.max(1), size.height.max(1));
        let surface = SurfaceTexture::new(self.surface.0, self.surface.1, window.clone());
        match Pixels::new(SCREEN_W as u32, SCREEN_H as u32, surface) {
            Ok(p) => {
                self.presenter = Some(Presenter::new(&p));
                self.pixels = Some(p);
            }
            Err(e) => return self.fail(event_loop, format!("cannot initialise the GPU surface: {e}")),
        }
        self.window = Some(window);
        // Do not count window/GPU start-up as game time.
        self.last_tick = Instant::now();
        self.start = self.last_tick;
        self.stats_since = self.last_tick;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(p) = self.pixels.as_mut() {
                        match p.resize_surface(size.width, size.height) {
                            Ok(()) => self.surface = (size.width, size.height),
                            Err(e) => eprintln!("pd-native: resize to {}x{} failed: {e}", size.width, size.height),
                        }
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::ModifiersChanged(m) => self.alt_held = m.state().alt_key(),
            WindowEvent::Focused(true) => self.focused = true,
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.input.clear(Source::Keyboard);
                self.input.set_mouse_buttons(Some(false), Some(false));
                self.alt_held = false;
                for sc in 0..=255u8 {
                    self.runner.key_event(sc, false);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let pressed = event.state == ElementState::Pressed;
                if pressed && !event.repeat {
                    let alt_enter = self.alt_held && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter);
                    if code == KeyCode::F11 || alt_enter {
                        self.toggle_fullscreen();
                        return;
                    }
                    if code == KeyCode::F10 {
                        self.integer = !self.integer;
                        return;
                    }
                }
                if let Some(button) = button_for(code) {
                    self.input.set(Source::Keyboard, button, pressed);
                }
                // Every key also goes to the app as the original's scancode
                // (menus, name entry, cheat codes). winit's `KeyCode` names
                // are the W3C `KeyboardEvent.code` values.
                if !event.repeat {
                    if let Some(sc) = pd_engine::input::scancode_from_key_code(&format!("{code:?}")) {
                        self.runner.key_event(sc, pressed);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Window position -> framebuffer pixel through the letterbox rectangle.
                let (rx, ry, rw, rh) = self.picture_rect();
                if rw > 0 && rh > 0 {
                    let x = (position.x - rx as f64) * SCREEN_W as f64 / rw as f64;
                    let y = (position.y - ry as f64) * SCREEN_H as f64 / rh as f64;
                    self.input.set_mouse_pos(x.floor() as i32, y.floor() as i32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => self.input.set_mouse_buttons(Some(pressed), None),
                    MouseButton::Right => self.input.set_mouse_buttons(None, Some(pressed)),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.pointer_locked {
                self.runner.mouse_motion(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            self.tick(event_loop);
        }
    }
}

/// Run until the window is closed or the app quits.
pub fn run(runner: Runner, probe: StageProbe, opts: Options) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("cannot start the event loop: {e}"))?;
    let audio = if opts.audio {
        match AudioOutput::open() {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("pd-native: running without sound: {e}");
                None
            }
        }
    } else {
        None
    };
    let now = Instant::now();
    let mut app = NativeApp {
        runner,
        integer: opts.integer,
        presenter: None,
        surface: (1, 1),
        opts,
        probe,
        audio,
        gamepads: Gamepads::new(),
        input: InputState::new(),
        window: None,
        pixels: None,
        alt_held: false,
        focused: true,
        pointer_locked: false,
        start: now,
        last_tick: now,
        error: None,
        seen_stage: None,
        stage_started: now,
        ticks: 0,
        frames: 0,
        stats_since: now,
    };
    event_loop.run_app(&mut app).map_err(|e| format!("event loop failed: {e}"))?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
