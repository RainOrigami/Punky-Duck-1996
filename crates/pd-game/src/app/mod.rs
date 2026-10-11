//! Program flow of the game: `pd_engine::App` for the whole of `DUCK.BAT`
//! (`INTRO.EXE`, `P.EXE`, `THE_END.EXE`).
//!
//! The flow follows `main` of `P.EXE` (`0x12050`, `docs/re/engine/states.md`).
//! Every blocking wait of the original is state advanced by [`Game::update`]
//! at the original's 70 Hz. Menu internals: `docs/re/engine/menus.md`.

pub mod core;
pub mod gfx;
pub mod intro;
pub mod keys;
pub mod menu;
pub mod play;
pub mod sound;
pub mod store;
pub mod the_end;

use pd_engine::runner::FixedStep;
use pd_engine::{App, AssetSource, Command, Framebuffer, Input, Storage};

use self::core::Core;
use self::gfx::{BLACK, FontState, blend_table};
use self::intro::Intro;
use self::menu::{Menu, MenuGlobals, Outcome};
use self::play::{Leave, World};
pub use self::store::Mode;
use self::the_end::TheEnd;

/// How the program is run; nothing here changes game rules.
#[derive(Clone, Debug)]
pub struct GameOptions {
    /// Original or enhanced behaviour (`crate::settings::Enhancements`).
    pub mode: Mode,
    /// Run `INTRO.EXE`'s movies first, as `DUCK.BAT` does.
    pub intro: bool,
    /// Run `THE_END.EXE`'s credits after EXIT, as `DUCK.BAT` does.
    pub credits: bool,
    /// 70 Hz ticks one rendered island frame takes. The original renders as
    /// fast as the machine can and runs `ticks + 1` simulation steps per
    /// frame, so this is the "machine speed": 8 is the DOSBox-X reference
    /// set-up (`docs/re/reference.md`), 4 a faster period machine.
    pub frame_ticks: u32,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self { mode: Mode::Original, intro: true, credits: true, frame_ticks: 4 }
    }
}

enum Stage {
    Boot,
    Intro(Intro),
    /// `menu_run` (`0x25b00`) is about to start.
    MenuOpen,
    Menu(Menu),
    /// The menu faded out; act on what it decided.
    MenuClosed(Outcome),
    /// The `NEWGAME` part of `main`: resets and island set-up.
    Setup(Outcome),
    /// `island_enter` and the rest before the frame loop.
    Enter,
    Island,
    /// The ending movies are queued.
    Ending,
    /// The transition picture to island `n` is queued.
    Transition(i32),
    GameOver(Menu),
    GameOverClosed(bool),
    TheEnd(TheEnd),
    Quit,
}

pub struct Game {
    core: Core,
    opts: GameOptions,
    step: FixedStep,
    stage: Stage,
    globals: MenuGlobals,
    world: World,
    quit_sent: bool,
}

impl Game {
    pub fn new(assets: Box<dyn AssetSource>, storage: Box<dyn Storage>, opts: GameOptions) -> Self {
        let world = World::new(opts.mode.enhancements(), opts.frame_ticks);
        Self {
            core: Core::new(assets, storage),
            opts,
            step: FixedStep::original(),
            stage: Stage::Boot,
            globals: MenuGlobals::default(),
            world,
            quit_sent: false,
        }
    }

    /// Missing or broken files noticed so far.
    pub fn errors(&self) -> &[String] {
        &self.core.errors
    }

    /// Flow events so far (`"menu"`, `"cd 4"`, `"island 1"`, `"movie PART_1.MOV"`, ...).
    pub fn trace(&self) -> &[String] {
        &self.core.trace
    }

    /// Name of the current stage, for traces.
    pub fn stage_name(&self) -> &'static str {
        match self.stage {
            Stage::Boot => "boot",
            Stage::Intro(_) => "intro",
            Stage::MenuOpen | Stage::Menu(_) | Stage::MenuClosed(_) => "menu",
            Stage::Setup(_) | Stage::Enter => "loading",
            Stage::Island => "island",
            Stage::Ending => "ending",
            Stage::Transition(_) => "transition",
            Stage::GameOver(_) | Stage::GameOverClosed(_) => "game over",
            Stage::TheEnd(_) => "credits",
            Stage::Quit => "quit",
        }
    }

    /// Sound effects requested so far as `(sample id, volume)`.
    pub fn sfx_log(&self) -> &[(usize, u8)] {
        &self.core.sound.log
    }

    pub fn settings(&self) -> &store::Settings {
        &self.core.settings
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// `menu_run` (`0x25b00`) up to the first pass of the main menu.
    fn menu_open(&mut self) {
        let core = &mut self.core;
        core.note("menu");
        // vesa_set_mode(0x101) in main: blank screen; palette black.
        core.video.fill_screen(0);
        core.video.set_palette(&BLACK);
        core.settings.load(core.storage.as_ref(), core.assets.as_ref());
        core.kbd.set_bios(true);
        core.kbd.flush();
        core.apply_sound();
        core.cd_stop();
        if core.settings.stp.music_on != 0 {
            core.cd_play(4);
        }
        core.cd_volume();
        let bg = core.load_picture("MENUSPIC.RON").unwrap_or(self::core::Picture {
            w: gfx::W,
            h: gfx::H,
            pixels: vec![0; gfx::W * gfx::H],
            pal: BLACK,
        });
        self.globals.dim = blend_table([0, 0, 0], 0x10, &bg.pal);
        core.coolfnt1 = core.load_sheet("COOLFNT1.SPT");
        core.coolfnt2 = core.load_sheet("COOLFNT2.SPT");
        core.coolfnt3 = core.load_sheet("COOLFNT3.SPT");
        core.font = FontState::set(&core.coolfnt2, 0x20, 0);
        core.video.clip_y0 = 0;
        core.video.clip_y1 = 0x1df;
        let pal = bg.pal;
        let mut menu = Menu::main(bg);
        menu.draw_first(core, &self.globals);
        core.fade(0x10, &BLACK, &pal);
        self.stage = Stage::Menu(menu);
    }

    /// `game_over_screen` (`0x255f0`). Returns the stage to go to.
    fn game_over(&mut self) -> Stage {
        let core = &mut self.core;
        core.kbd.set_bios(true);
        core.kbd.flush();
        if !self.globals.player_mode {
            self.globals.in_progress = false;
            self.world.sim.s.p.lives = -1;
            return Stage::MenuOpen;
        }
        if self.world.sim.s.p.lives >= 0 {
            self.globals.in_progress = true;
            return Stage::MenuOpen;
        }
        self.globals.in_progress = false;
        core.note("game over");
        core.video.fill_screen(0);
        core.video.set_palette(&BLACK);
        let n = self.world.sim.s.island;
        let bg = core.load_picture(&format!("GO_{n}.RON")).unwrap_or(self::core::Picture {
            w: gfx::W,
            h: gfx::H,
            pixels: vec![0; gfx::W * gfx::H],
            pal: BLACK,
        });
        core.coolfnt1 = core.load_sheet("COOLFNT1.SPT");
        core.coolfnt2 = core.load_sheet("COOLFNT2.SPT");
        core.coolfnt3 = core.load_sheet("COOLFNT3.SPT");
        core.font = FontState::set(&core.coolfnt2, 0x20, 0);
        core.video.clip_y0 = 0;
        core.video.clip_y1 = 0x1df;
        let pal = bg.pal;
        let mut menu = Menu::game_over(bg, self.world.sim.s.p.continues);
        menu.draw_first(core, &self.globals);
        core.fade(0x10, &BLACK, &pal);
        Stage::GameOver(menu)
    }

    fn quit_stage(&mut self) -> Stage {
        // Text mode, CD stop (0x12086).
        self.core.video.fill_screen(0);
        self.core.video.set_palette(&BLACK);
        self.core.cd_stop();
        if self.opts.credits { Stage::TheEnd(TheEnd::new(&mut self.core)) } else { Stage::Quit }
    }

    /// One 70 Hz tick.
    fn tick(&mut self) {
        self.core.ticks += 1;
        let input = self.core.input;
        self.core.kbd.tick(&input);
        if let Stage::Island = self.stage {
            // The island loop keeps its own tick count while waits run.
            let busy = self.core.run_ops();
            let leave = self.world.tick(&mut self.core);
            if !busy {
                if let Some(leave) = leave {
                    self.leave_island(leave);
                }
            }
            return;
        }
        if self.core.run_ops() {
            return;
        }
        match std::mem::replace(&mut self.stage, Stage::Quit) {
            Stage::Boot => {
                self.stage = if self.opts.intro { Stage::Intro(Intro::new()) } else { Stage::MenuOpen };
            }
            Stage::Intro(i) => {
                self.stage = if i.is_done() { Stage::MenuOpen } else { Stage::Intro(i) };
            }
            Stage::MenuOpen => self.menu_open(),
            Stage::Menu(mut menu) => {
                menu.tick(&mut self.core, &mut self.globals, &mut self.world);
                if menu.is_done() {
                    let pal = menu.bg.pal;
                    self.core.fade(0x10, &pal, &BLACK);
                    // settings_save (0x25a40)
                    self.core.then(|core| core.settings.save_stp(core.storage.as_mut()));
                    self.stage = Stage::MenuClosed(menu.out);
                } else {
                    self.stage = Stage::Menu(menu);
                }
            }
            Stage::MenuClosed(out) => {
                self.stage = if out.quit { self.quit_stage() } else { Stage::Setup(out) };
            }
            Stage::Setup(out) => {
                let mut ok = true;
                if out.new_game {
                    self.world.new_game(&self.core);
                    self.world.pending_load = None;
                }
                if !out.resume {
                    let island = if out.new_game { out.island } else { self.world.sim.s.island };
                    ok = self.world.start_island(&mut self.core, island);
                } else if let Some(data) = self.world.pending_load.take() {
                    ok = self.world.prepare_loaded(&mut self.core, &data);
                }
                self.stage = if ok { Stage::Enter } else { Stage::MenuOpen };
            }
            Stage::Enter => {
                self.stage = if self.world.enter(&mut self.core) { Stage::Island } else { Stage::MenuOpen };
            }
            Stage::Island => unreachable!("handled above"),
            Stage::Ending => {
                // 0x125d6: next_island = 0, lives = -1, cd_stop, kbd_remove.
                self.world.sim.s.next_island = 0;
                self.world.sim.s.p.lives = -1;
                self.core.cd_stop();
                self.core.kbd.set_bios(true);
                self.stage = Stage::MenuOpen;
            }
            Stage::Transition(n) => {
                // 0x12669: island = next, not resumed.
                self.world.sim.s.island = n;
                self.core.kbd.set_bios(true);
                self.stage = Stage::Setup(Outcome { island: n, ..Outcome::default() });
            }
            Stage::GameOver(mut menu) => {
                menu.tick(&mut self.core, &mut self.globals, &mut self.world);
                if menu.is_done() {
                    let pal = menu.bg.pal;
                    self.core.fade(0x10, &pal, &BLACK);
                    self.stage = Stage::GameOverClosed(menu.out.continue_game);
                } else {
                    self.stage = Stage::GameOver(menu);
                }
            }
            Stage::GameOverClosed(cont) => {
                if cont {
                    self.world.sim.continue_game();
                    self.stage = Stage::Enter;
                } else {
                    self.stage = Stage::MenuOpen;
                }
            }
            Stage::TheEnd(mut t) => {
                t.tick(&mut self.core);
                self.stage = if t.is_done() { Stage::Quit } else { Stage::TheEnd(t) };
            }
            Stage::Quit => {
                if !self.quit_sent {
                    self.quit_sent = true;
                    self.core.commands.push(Command::Quit);
                }
            }
        }
    }

    /// After the frame loop of `main` (`0x124ea`).
    fn leave_island(&mut self, leave: Leave) {
        self.world.left();
        self.core.note(format!("leave island: {leave:?}"));
        match leave {
            Leave::Ending => {
                let core = &mut self.core;
                core.cd_stop();
                core.cd_play(7);
                core.cd_stop();
                core.cd_play(7);
                let from = self.world.animaps_pal();
                core.fade(0x20, &from, &BLACK);
                core.then(|core| {
                    core.video.fill_screen(0);
                    core.video.clip_y0 = 0;
                    core.video.clip_y1 = 0x1df;
                    core.video.set_palette(&BLACK);
                });
                core.play_flc("PART_1.MOV", 15);
                core.play_flc("PART_2.MOV", 1);
                self.stage = Stage::Ending;
            }
            Leave::NextIsland(n) => {
                self.world.transition_picture(&mut self.core, n);
                self.stage = Stage::Transition(n);
            }
            Leave::Escape | Leave::GameOver => {
                self.stage = self.game_over();
            }
        }
    }
}

impl App for Game {
    fn update(&mut self, dt_us: u64, input: &Input, commands: &mut Vec<Command>) {
        self.core.input = *input;
        let ticks = self.step.advance(dt_us);
        for _ in 0..ticks {
            self.tick();
            self.core.prev_input = self.core.input;
        }
        // INTRO.EXE runs on its own millisecond timer.
        if let Stage::Intro(intro) = &mut self.stage {
            intro.update(dt_us, &mut self.core);
        }
        commands.append(&mut self.core.commands);
    }

    fn render(&mut self, fb: &mut Framebuffer) {
        self.core.video.show(fb);
    }

    fn mix_audio(&mut self, out: &mut [i16]) {
        self.core.sound.mix(out);
    }

    fn key_event(&mut self, scancode: u8, pressed: bool) {
        self.core.kbd.key_event(scancode, pressed);
    }

    fn mouse_motion(&mut self, dx: f32, _dy: f32) {
        if self.wants_pointer_lock() {
            self.core.mouse_dx += dx;
        }
    }

    fn wants_pointer_lock(&self) -> bool {
        matches!(self.stage, Stage::Island) && self.world.wants_mouse()
    }

    fn cd_status(&mut self, playing: bool) {
        // A play command issued this update has not reached the player yet.
        if self.core.commands.is_empty() {
            self.core.cd_playing = playing;
        }
    }
}
