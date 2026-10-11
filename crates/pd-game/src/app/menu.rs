//! The menus of `P.EXE` (`0x25470`–`0x27a00`) and the game-over screen
//! (`0x255f0`). Details and evidence: `docs/re/engine/menus.md`.
//!
//! Every page of the original is a loop of *draw page, read one key, act*.
//! Here a page is an entry on a stack and one loop pass is [`Menu::iterate`].
//! The original loop is not throttled; its pass rate on the reference
//! machine (DOSBox-X, `docs/re/reference.md`) is 33 retraces per 8 passes.

use pd_formats::stp::{DIFFICULTIES, LANGUAGES};

use super::core::{Core, Picture};
use super::gfx::{self, Align};
use super::keys::BiosKey;
use super::sound::{SFX_MOVE, SFX_SELECT};
use super::store::{SLOTS, slot_key};

/// What the menus need from the game state.
pub trait SaveGame {
    /// Current island 1..=5 (`0x517f88`), for the default slot name.
    fn island(&self) -> i32;
    /// The slot file contents (`0x26100`), `None` if there is no state to save.
    fn save(&self) -> Option<Vec<u8>>;
    /// Restore from a slot file (`0x26050`). `false` if the data is unusable.
    fn load(&mut self, data: &[u8]) -> bool;
}

/// Menu variables that outlive one visit to the menu (statics of `P.EXE`).
pub struct MenuGlobals {
    /// Last 22 typed characters (`0x416ca`) and the latch (`0x416e0`).
    pub cheat_buf: [u8; 22],
    pub cheat: bool,
    /// A game is in progress: RESUME GAME / SAVE GAME are offered (`0x50d5e3`).
    pub in_progress: bool,
    /// `0x41628`: 0 = attract (demo playback) mode, 1 after NEW GAME / LOAD.
    pub player_mode: bool,
    /// The box darkening table (`0x50cab4`), built from `MENUSPIC.RON`'s
    /// palette each time the main menu opens and *not* rebuilt for the
    /// game-over pictures.
    pub dim: [u8; 256],
}

impl Default for MenuGlobals {
    fn default() -> Self {
        Self { cheat_buf: [b' '; 22], cheat: false, in_progress: false, player_mode: false, dim: [0; 256] }
    }
}

pub const CHEAT_PHRASE: &[u8; 22] = b"THE TRUTH IS OUT THERE";

const MAIN_ITEMS: [&[u8]; 4] = [b"NEW GAME", b"LOAD GAME", b"OPTIONS", b"EXIT"];
const MAIN_ITEMS_GAME: [&[u8]; 6] = [b"NEW GAME", b"LOAD GAME", b"RESUME GAME", b"SAVE GAME", b"OPTIONS", b"EXIT"];
const RESOLUTIONS: [&[u8]; 3] = [b"VGA 320X200", b"SVGA 320X200", b"SVGA 640X400"];
const FAST_MODES: [&[u8]; 2] = [b"FAST MODE OFF", b"FAST MODE ON"];
const SOUND_NAMES: [&[u8]; 2] = [b"SOUND OFF", b"SOUND ON"];
const MUSIC_NAMES: [&[u8]; 2] = [b"MUSIC OFF", b"MUSIC ON"];
const CONTROLLERS: [&[u8]; 2] = [b"KEYBOARD", b"JOYSTICK"];
const ISLANDS: [&[u8]; 6] =
    [b"FISHER ISLAND", b"OASIS ISLAND", b"YETI ISLAND", b"COLD ISLAND", b"SMOKE ISLAND", b"CANCEL"];
/// Characters accepted in a slot name (`0x41788`).
const NAME_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ";

/// Loop-pass clock: units per tick and per pass (33 retraces per 8 passes).
const PASS_NUM: u32 = 8;
const PASS_DEN: u32 = 33;

#[derive(Clone, Debug, PartialEq)]
enum Page {
    Main { sel: i32 },
    Cheat { sel: i32 },
    Load { sel: i32 },
    LoadConfirm { slot: usize, sel: i32 },
    Save { sel: i32 },
    Name { slot: usize, text: Vec<u8>, count: u32 },
    /// After the "GAME SAVED" message: write `SAVEGAME.REC`.
    SaveDone,
    Options { sel: i32 },
    Resolution { sel: i32 },
    Sound { sel: i32 },
    Controller { sel: i32 },
    Exit { sel: i32 },
    /// A message drawn once; any key continues.
    Message { title: &'static [u8], lines: &'static [&'static [u8]], shown: bool },
    /// Game over without continues: the picture, any character key leaves.
    GameOverWait { sel: i32 },
    /// `CONTINUE 9` countdown.
    Continue { sel: i32, count: i32 },
}

/// What a visit to the main menu decided (`0x50d5e0`..`0x50d5e2`, `0x517f88`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    pub new_game: bool,
    /// Island chosen for a new game.
    pub island: i32,
    /// Continue with the state in memory: RESUME GAME or a loaded save.
    pub resume: bool,
    pub quit: bool,
    /// Game-over screen: the player chose to continue.
    pub continue_game: bool,
}

pub struct Menu {
    pages: Vec<Page>,
    phase: u32,
    /// Background picture (`0x2c4498`) and its palette (`0x50cbc4`).
    pub bg: Picture,
    pub out: Outcome,
    /// `0x50d5e1`: 0 no, 1 quit, 2 undecided (EXIT GAME ? page open).
    quit: u8,
    /// The page loop of the top-level function has ended.
    done: bool,
}

fn cycle(value: &mut u32, n: u32) {
    // 0x25e60 with direction +1.
    *value = value.wrapping_add(1);
    if *value == n {
        *value = 0;
    }
}

fn name_of<'a>(names: &[&'a [u8]], index: u32) -> &'a [u8] {
    // Out-of-range values (a hand-edited GAME.STP) index past the pointer
    // table in the original; show nothing instead.
    names.get(index as usize).copied().unwrap_or(b"")
}

impl Menu {
    /// The main menu (`menu_run` after its set-up, `0x25ed0`).
    pub fn main(bg: Picture) -> Self {
        Self { pages: vec![Page::Main { sel: 0 }], phase: 0, bg, out: Outcome::default(), quit: 0, done: false }
    }

    /// The game-over screen (`0x255f0`, lives below zero): `GO_n.RON`, then
    /// either a wait for a key or the continue countdown.
    pub fn game_over(bg: Picture, continues: i32) -> Self {
        let page = if continues == 0 { Page::GameOverWait { sel: 0 } } else { Page::Continue { sel: 0, count: 599 } };
        Self { pages: vec![page], phase: 0, bg, out: Outcome::default(), quit: 0, done: false }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// `menu_draw(title, items, selected)` (`0x25470`).
    fn draw(&self, core: &mut Core, g: &MenuGlobals, title: &[u8], items: &[&[u8]], sel: i32) {
        let h = items.len() as i32 * 0x28 + 0x6e;
        let top = -(h / 2);
        core.video.blit_picture(&self.bg.pixels, self.bg.w, self.bg.h);
        core.video.remap_box(&g.dim, 400, h);
        let font = core.font;
        let item_sheet = if sel < 0 { &core.coolfnt1 } else { &core.coolfnt3 };
        for (i, item) in items.iter().enumerate() {
            core.video.text(item_sheet, &font, 0x145, i as i32 * 0x28 + top + 0x168, item, Align::Centre);
        }
        core.video.text(&core.coolfnt2, &font, 0x145, top + 0x118, title, Align::Centre);
        if sel >= 0 {
            if let Some(item) = items.get(sel as usize) {
                core.video.text(&core.coolfnt1, &font, 0x145, sel * 0x28 + top + 0x168, item, Align::Centre);
            }
        }
        core.video.present();
    }

    /// The picture alone (game over without continues, `0x25700`).
    fn draw_picture(&self, core: &mut Core) {
        core.video.blit_picture(&self.bg.pixels, self.bg.w, self.bg.h);
        core.video.present();
    }

    /// `menu_input(&sel, count)` (`0x25c90`).
    fn input(&self, core: &mut Core, g: &mut MenuGlobals, sel: &mut i32, count: i32) -> BiosKey {
        let mut key = BiosKey::default();
        while let Some(k) = core.kbd.getkey() {
            key = k;
        }
        if key.ascii as u32 + key.scan as u32 != 0 {
            core.kbd.flush();
        }
        if key.scan == 0x48 {
            *sel -= 1;
        }
        if *sel < 0 {
            *sel = count - 1;
        }
        if key.scan == 0x50 {
            *sel += 1;
        }
        if count == *sel {
            *sel = 0;
        }
        if *sel < 0 {
            *sel = 0;
        }
        if key.ascii == 0x1b {
            *sel = count - 1;
            key.ascii = 0x0d;
        }
        if matches!(key.scan, 0x50 | 0x4d | 0x4b | 0x48) {
            core.sound.play(SFX_MOVE, 0xff);
        }
        if key.ascii == 0x0d {
            core.sound.play(SFX_SELECT, 0xff);
        }
        if key.ascii != 0 && !g.cheat {
            g.cheat_buf.copy_within(1.., 0);
            g.cheat_buf[21] = key.ascii;
            if &g.cheat_buf == CHEAT_PHRASE {
                g.cheat = true;
                core.note("cheat");
                // Flash: to the red palette in 4 steps, back in 16.
                let pal = self.bg.pal;
                let red = gfx::red_palette(&pal);
                core.fade(4, &pal, &red);
                core.fade(0x10, &red, &pal);
            }
        }
        key
    }

    /// One tick. Runs a loop pass when one is due and nothing blocks.
    pub fn tick(&mut self, core: &mut Core, g: &mut MenuGlobals, world: &mut dyn SaveGame) {
        if self.done || core.busy() {
            return;
        }
        self.phase += PASS_NUM;
        if self.phase < PASS_DEN {
            return;
        }
        self.phase -= PASS_DEN;
        self.iterate(core, g, world);
    }

    /// Draw the current page without reading input (the draw before the
    /// first fade-in).
    pub fn draw_first(&mut self, core: &mut Core, g: &MenuGlobals) {
        match self.pages.last() {
            Some(Page::Main { sel }) => {
                let sel = *sel;
                self.draw_main(core, g, sel);
            }
            Some(Page::Continue { sel, count }) => {
                let (sel, count) = (*sel, *count);
                self.draw_continue(core, g, sel, count);
            }
            Some(Page::GameOverWait { .. }) => self.draw_picture(core),
            _ => {}
        }
    }

    fn draw_main(&self, core: &mut Core, g: &MenuGlobals, sel: i32) {
        if g.in_progress {
            self.draw(core, g, b"MAIN MENU", &MAIN_ITEMS_GAME, sel);
        } else {
            self.draw(core, g, b"MAIN MENU", &MAIN_ITEMS, sel);
        }
    }

    fn draw_continue(&self, core: &mut Core, g: &MenuGlobals, sel: i32, count: i32) {
        let mut title = *b"CONTINUE 9";
        title[9] = b'0'.wrapping_add((count / 60) as u8);
        self.draw(core, g, &title, &[b"YES", b"NO"], sel);
    }

    fn slot_items(core: &Core) -> Vec<Vec<u8>> {
        let mut items: Vec<Vec<u8>> = (0..SLOTS).map(|i| core.settings.slot_name(i).to_vec()).collect();
        items.push(b"CANCEL".to_vec());
        items
    }

    /// One pass of the loop of the page on top of the stack.
    pub fn iterate(&mut self, core: &mut Core, g: &mut MenuGlobals, world: &mut dyn SaveGame) {
        let Some(mut page) = self.pages.pop() else {
            self.done = true;
            return;
        };
        // `keep`: the page stays; `push`: a sub-page opened on top of it.
        let mut keep = true;
        let mut push: Option<Page> = None;
        // The page returned without a pass of its own: run the caller's now.
        let mut again = false;
        match &mut page {
            Page::Main { sel } => {
                self.draw_main(core, g, *sel);
                let count = if g.in_progress { 6 } else { 4 };
                let key = self.input(core, g, sel, count);
                if key.ascii == 0x0d {
                    let choice = match (g.in_progress, *sel) {
                        (_, 0) => 0,
                        (_, 1) => 1,
                        (false, 2) | (true, 4) => 4,
                        (false, 3) | (true, 5) => 5,
                        (true, n) => n,
                        _ => -1,
                    };
                    match choice {
                        0 => {
                            // menu_new_game (0x27880)
                            if g.cheat {
                                push = Some(Page::Cheat { sel: 0 });
                            } else {
                                self.out.island = 1;
                                g.player_mode = true;
                                self.out.new_game = true;
                            }
                        }
                        1 => push = Some(Page::Load { sel: 0 }),
                        2 => self.out.resume = true,
                        3 => push = Some(Page::Save { sel: 0 }),
                        4 => push = Some(Page::Options { sel: 0 }),
                        5 => {
                            self.quit = 2;
                            push = Some(Page::Exit { sel: 0 });
                        }
                        _ => {}
                    }
                }
                if push.is_none() && (self.out.new_game || self.out.resume || self.quit != 0) {
                    keep = false;
                }
            }
            Page::Cheat { sel } => {
                self.draw(core, g, b"CHEAT MODE", &ISLANDS, *sel);
                let key = self.input(core, g, sel, 6);
                if key.ascii == 0x0d {
                    if (0..5).contains(sel) {
                        self.out.island = *sel + 1;
                        g.player_mode = true;
                        self.out.new_game = true;
                    } else {
                        self.out.new_game = false;
                    }
                    keep = false;
                }
            }
            Page::Load { sel } => {
                let items = Self::slot_items(core);
                let refs: Vec<&[u8]> = items.iter().map(Vec::as_slice).collect();
                self.draw(core, g, b"LOAD GAME", &refs, *sel);
                let key = self.input(core, g, sel, 9);
                if key.ascii == 0x0d {
                    if *sel == 8 {
                        keep = false;
                    } else if (0..8).contains(sel) {
                        push = Some(Page::LoadConfirm { slot: *sel as usize, sel: 0 });
                    }
                }
            }
            Page::LoadConfirm { slot, sel } => {
                // 0x26460: "LOAD <name> ?" with NO / YES; Esc lands on YES.
                let mut title = b"LOAD ".to_vec();
                title.extend_from_slice(core.settings.slot_name(*slot));
                title.extend_from_slice(b" ?");
                self.draw(core, g, &title, &[b"NO", b"YES"], *sel);
                let key = self.input(core, g, sel, 2);
                if key.ascii == 0x0d {
                    keep = false;
                    if *sel != 0 {
                        // savegame_read (0x26050)
                        let data = core.storage.read(&slot_key(*slot));
                        let loaded = match data {
                            Some(d) => {
                                let ok = world.load(&d);
                                if ok && d.len() >= 4 {
                                    // The last field of the file is the difficulty (0x41624).
                                    let t = &d[d.len() - 4..];
                                    core.settings.stp.difficulty = u32::from_le_bytes([t[0], t[1], t[2], t[3]]);
                                }
                                ok
                            }
                            None => false,
                        };
                        if loaded {
                            core.note(format!("load slot {slot}"));
                            self.out.resume = true;
                            g.player_mode = true;
                            // Leave the LOAD GAME list too.
                            self.pages.pop();
                        } else {
                            push = Some(Page::Message {
                                title: b"!!! ERROR !!!",
                                lines: &[b"GAME NOT", b"LOADED !"],
                                shown: false,
                            });
                        }
                    }
                }
            }
            Page::Save { sel } => {
                let items = Self::slot_items(core);
                let refs: Vec<&[u8]> = items.iter().map(Vec::as_slice).collect();
                self.draw(core, g, b"SAVE GAME", &refs, *sel);
                let key = self.input(core, g, sel, 9);
                if key.ascii == 0x0d {
                    if *sel == 8 {
                        keep = false;
                    } else if (0..8).contains(sel) {
                        let mut text = core.settings.slot_name(*sel as usize).to_vec();
                        text.push(b'+');
                        push = Some(Page::Name { slot: *sel as usize, text, count: 0 });
                    }
                }
            }
            Page::Name { slot, text, count } => {
                // menu_enter_name (0x26280). `text` ends with the cursor character.
                *count += 1;
                let mut sel = 0;
                let mut key = self.input(core, g, &mut sel, 0);
                if key.ascii.is_ascii_lowercase() {
                    key.ascii -= 0x20;
                }
                let len = text.len();
                if let Some(last) = text.last_mut() {
                    *last = b'+' - ((*count & 4) >> 2) as u8;
                }
                self.draw(core, g, b"ENTER NEW NAME", &[text.as_slice()], 0);
                if key.scan == 0x0e && len > 1 {
                    text.pop();
                    if let Some(last) = text.last_mut() {
                        *last = b',';
                    }
                }
                if key.ascii != 0 && NAME_CHARS.contains(&key.ascii) && len < 9 {
                    // Written at the positions computed before a backspace
                    // in the same pass; both cannot happen together.
                    if let Some(last) = text.last_mut() {
                        *last = key.ascii;
                    }
                    text.push(b',');
                }
                if sel < 0 || key.ascii == 0x0d {
                    keep = false;
                    text.pop();
                    if text.is_empty() {
                        // sprintf("ISLAND%#02d", island)
                        *text = format!("ISLAND{:02}", world.island()).into_bytes();
                    }
                    if sel >= 0 {
                        core.settings.set_slot_name(*slot, text);
                        // savegame_write (0x26100)
                        match world.save() {
                            Some(data) => {
                                core.storage.write(&slot_key(*slot), &data);
                                core.note(format!("save slot {slot}"));
                                self.pages.push(Page::SaveDone);
                                push = Some(Page::Message {
                                    title: b"OK !",
                                    lines: &[b"GAME SAVED", b"SUCCESSFULLY !"],
                                    shown: false,
                                });
                            }
                            None => {
                                push = Some(Page::Message {
                                    title: b"!!! ERROR !!!",
                                    lines: &[b"GAME NOT", b"SAVED !"],
                                    shown: false,
                                });
                            }
                        }
                    }
                }
            }
            Page::SaveDone => {
                core.settings.save_rec(core.storage.as_mut());
                keep = false;
                again = true;
            }
            Page::Options { sel } => {
                let speed = core.settings.stp.game_speed;
                let mut bar = *b"GAME SPEED : .............";
                for (i, c) in bar[13..].iter_mut().enumerate() {
                    *c = if (i as i32) <= speed + 6 { b'+' } else { b'*' };
                }
                let items: [&[u8]; 7] = [
                    name_of(&DIFFICULTIES.map(str::as_bytes), core.settings.stp.difficulty),
                    name_of(&LANGUAGES.map(str::as_bytes), core.settings.stp.language),
                    &bar,
                    b"RESOLUTION OPTIONS",
                    b"SOUND OPTIONS",
                    b"CONTROLLER OPTIONS",
                    b"RETURN",
                ];
                self.draw(core, g, b"OPTIONS", &items, *sel);
                let key = self.input(core, g, sel, 7);
                let stp = &mut core.settings.stp;
                if key.ascii == 0 {
                    if *sel == 2 {
                        if key.scan == 0x4b && stp.game_speed > -6 {
                            stp.game_speed -= 1;
                        } else if key.scan == 0x4d && stp.game_speed < 6 {
                            stp.game_speed += 1;
                        }
                    }
                } else if key.ascii == 0x0d {
                    match *sel {
                        0 => cycle(&mut stp.difficulty, 3),
                        1 => cycle(&mut stp.language, 5),
                        2 => {
                            stp.game_speed += 1;
                            if stp.game_speed == 7 {
                                stp.game_speed = 6;
                            }
                        }
                        3 => push = Some(Page::Resolution { sel: 0 }),
                        4 => push = Some(Page::Sound { sel: 0 }),
                        5 => push = Some(Page::Controller { sel: 0 }),
                        6 => keep = false,
                        _ => {}
                    }
                }
            }
            Page::Resolution { sel } => {
                let s = &core.settings;
                let items: [&[u8]; 3] = [name_of(&RESOLUTIONS, s.view_mode), name_of(&FAST_MODES, s.fast_mode), b"RETURN"];
                self.draw(core, g, b"RESOLUTION", &items, *sel);
                let key = self.input(core, g, sel, 3);
                let s = &mut core.settings;
                if key.ascii == 0x0d {
                    match *sel {
                        0 => {
                            cycle(&mut s.view_mode, 3);
                            if s.view_mode != 2 && s.fast_mode == 1 {
                                s.fast_mode = 0;
                            }
                        }
                        1 => {
                            cycle(&mut s.fast_mode, 2);
                            if s.fast_mode != 0 {
                                s.view_mode = 2;
                            }
                        }
                        2 => keep = false,
                        _ => {}
                    }
                }
            }
            Page::Sound { sel } => {
                let stp = core.settings.stp;
                let mut sfx = *b"SFX VOLUME : ,,,,,,,,,,,,,,,,";
                for (i, c) in sfx[13..].iter_mut().enumerate() {
                    *c = if (i as i32) <= stp.sfx_volume as i32 { b'+' } else { b'*' };
                }
                let mut cd = *b"CD VOLUME : ,,,,,,,,,,,,,,,,";
                for (i, c) in cd[12..].iter_mut().enumerate() {
                    *c = if (i as i32) <= stp.cd_volume as i32 { b'+' } else { b'*' };
                }
                let items: [&[u8]; 5] =
                    [name_of(&SOUND_NAMES, stp.sound_on), name_of(&MUSIC_NAMES, stp.music_on), &sfx, &cd, b"RETURN"];
                self.draw(core, g, b"SOUND AND MUSIC", &items, *sel);
                let key = self.input(core, g, sel, 5);
                let before = core.settings.stp;
                let stp = &mut core.settings.stp;
                let mut restart_music = false;
                if key.ascii == 0 {
                    let value = match *sel {
                        2 => Some(&mut stp.sfx_volume),
                        3 => Some(&mut stp.cd_volume),
                        _ => None,
                    };
                    if let Some(v) = value {
                        if key.scan == 0x4b && *v != 0 {
                            *v -= 1;
                        } else if key.scan == 0x4d && (*v as i32) < 0xf {
                            *v += 1;
                        }
                    }
                } else if key.ascii == 0x0d {
                    match *sel {
                        0 => cycle(&mut stp.sound_on, 2),
                        1 => {
                            cycle(&mut stp.music_on, 2);
                            restart_music = true;
                        }
                        2 => stp.sfx_volume = stp.sfx_volume.wrapping_add(1) & 0xf,
                        3 => stp.cd_volume = stp.cd_volume.wrapping_add(1) & 0xf,
                        4 => keep = false,
                        _ => {}
                    }
                }
                if restart_music {
                    core.cd_stop();
                    if core.settings.stp.music_on != 0 {
                        core.cd_play(4);
                    }
                }
                // The original re-applies these on every pass.
                core.apply_sound();
                if core.settings.stp.cd_volume != before.cd_volume {
                    core.cd_volume();
                }
            }
            Page::Controller { sel } => {
                let items: [&[u8]; 3] =
                    [name_of(&CONTROLLERS, core.settings.stp.controller), b"CALIBRATE JOYSTICK", b"RETURN"];
                self.draw(core, g, b"CONTROLLER", &items, *sel);
                let key = self.input(core, g, sel, 3);
                if key.ascii == 0x0d {
                    match *sel {
                        0 => cycle(&mut core.settings.stp.controller, 2),
                        1 => {
                            // 0x26c70: joy_detect(0) fails without a game port.
                            push = Some(Page::Message {
                                title: b"CALIBRATE JOYSTICK",
                                lines: &[b"NO JOYSTICK", b"DETECTED !", b"PRESS A KEY..."],
                                shown: false,
                            });
                        }
                        2 => {
                            keep = false;
                            // 0x26c50: no joystick (0x4193c set) -> back to keyboard.
                            core.settings.stp.controller = 0;
                        }
                        _ => {}
                    }
                }
            }
            Page::Exit { sel } => {
                self.draw(core, g, b"EXIT GAME ?", &[b"HO NO!", b"YES, SORRY..."], *sel);
                let key = self.input(core, g, sel, 2);
                if key.ascii == 0x0d {
                    self.quit = if *sel == 1 { 1 } else { 0 };
                    keep = false;
                }
            }
            Page::Message { title, lines, shown } => {
                if !*shown {
                    self.draw(core, g, title, lines, -1);
                    *shown = true;
                    // bios_getkey (0x359a3) blocks; the page closes on the next pass.
                    core.wait_key();
                } else {
                    keep = false;
                    again = true;
                }
            }
            Page::GameOverWait { sel } => {
                let key = self.input(core, g, sel, 1);
                if key.ascii != 0 {
                    keep = false;
                }
            }
            Page::Continue { sel, count } => {
                self.draw_continue(core, g, *sel, *count);
                *count -= 1;
                let key = self.input(core, g, sel, 2);
                if key.ascii == 0x0d {
                    self.out.continue_game = *sel == 0;
                    keep = false;
                }
                if *count == 0 {
                    self.out.continue_game = false;
                    keep = false;
                }
            }
        }
        if keep {
            self.pages.push(page);
        }
        if let Some(p) = push {
            self.pages.push(p);
        }
        // The main menu loop ends once a result is set (0x26010).
        if let [Page::Main { .. }] = self.pages.as_slice() {
            if self.out.new_game || self.out.resume || self.quit == 1 {
                self.pages.clear();
            }
        }
        if self.pages.is_empty() {
            self.out.quit = self.quit == 1;
            self.done = true;
        } else if again {
            self.iterate(core, g, world);
        }
    }
}
