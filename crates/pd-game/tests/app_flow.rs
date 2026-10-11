//! Program flow tests of `pd_game::app::Game` against the original disc files
//! (`disc/extracted/PUNKY`). Skipped when the files are absent.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use pd_engine::{App, AssetSource, Command, Framebuffer, Input, Storage};
use pd_game::app::{Game, GameOptions, Mode};
use pd_game::sim::Event;

struct DirAssets(PathBuf);

impl AssetSource for DirAssets {
    fn load(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(name.to_ascii_uppercase())).ok()
    }
}

#[derive(Clone, Default)]
struct SharedStorage(Rc<RefCell<BTreeMap<String, Vec<u8>>>>);

impl Storage for SharedStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        self.0.borrow().get(key).cloned()
    }
    fn write(&mut self, key: &str, data: &[u8]) {
        self.0.borrow_mut().insert(key.to_string(), data.to_vec());
    }
}

struct Rig {
    game: Game,
    storage: SharedStorage,
    commands: Vec<Command>,
    ticks: u64,
    /// Buttons held, as the frontends report them next to the raw keys.
    held: Input,
}

fn disc_dir() -> Option<PathBuf> {
    let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY"));
    p.join("BANQUE.BNK").is_file().then_some(p)
}

impl Rig {
    fn new(opts: GameOptions, storage: SharedStorage) -> Option<Rig> {
        let dir = disc_dir()?;
        let game = Game::new(Box::new(DirAssets(dir)), Box::new(storage.clone()), opts);
        Some(Rig { game, storage, commands: Vec::new(), ticks: 0, held: Input::default() })
    }

    fn plain() -> Option<Rig> {
        Rig::new(GameOptions { intro: false, credits: false, frame_ticks: 8, mode: Mode::Original }, SharedStorage::default())
    }

    fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            let held = self.held;
            self.game.update(14_286, &held, &mut self.commands);
            self.ticks += 1;
        }
    }

    /// Run until the stage has this name; panics after `limit` ticks.
    fn until(&mut self, stage: &str, limit: u64) {
        for _ in 0..limit {
            if self.game.stage_name() == stage {
                return;
            }
            self.run(1);
        }
        panic!("stage {stage} not reached; at {} after {limit} ticks; trace {:?}", self.game.stage_name(), self.game.trace());
    }

    fn tap(&mut self, scancode: u8) {
        let set = |i: &mut Input, on: bool| match scancode {
            0x39 => i.secondary = on,
            0x01 => i.cancel = on,
            0x1c => i.item = on,
            _ => {}
        };
        self.game.key_event(scancode, true);
        set(&mut self.held, true);
        self.run(12);
        self.game.key_event(scancode, false);
        set(&mut self.held, false);
        self.run(28);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let sc = match c {
                ' ' => 0x39,
                c => {
                    const ROWS: [(&str, u8); 3] = [("QWERTYUIOP", 0x10), ("ASDFGHJKL", 0x1e), ("ZXCVBNM", 0x2c)];
                    ROWS.iter().find_map(|(row, base)| row.find(c).map(|i| base + i as u8)).expect("letter")
                }
            };
            self.game.key_event(0x2a, true);
            self.game.key_event(sc, true);
            self.run(6);
            self.game.key_event(sc, false);
            self.game.key_event(0x2a, false);
            self.run(6);
        }
    }

    fn traced(&self, what: &str) -> bool {
        self.game.trace().iter().any(|t| t == what)
    }

    fn picture_is_black(&mut self) -> bool {
        let mut fb = Framebuffer::new();
        self.game.render(&mut fb);
        fb.pixels.iter().all(|&p| fb.palette[p as usize] == [0, 0, 0])
    }
}

const UP: u8 = 0xc8;
const DOWN: u8 = 0xd0;
const LEFT: u8 = 0xcb;
const RIGHT: u8 = 0xcd;
const ENTER: u8 = 0x1c;
const ESC: u8 = 0x01;

#[test]
fn options_are_written_to_game_stp_like_the_original() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    assert_eq!(r.game.stage_name(), "menu");
    assert!(r.commands.contains(&Command::PlayCdTrack { track: 4, looping: false }));
    // The key sequence of the DOSBox-X reference run (out/app-ref/a): the
    // original then wrote 1,1,0,15,5,2,2,1.
    for k in [
        DOWN, DOWN, ENTER, DOWN, DOWN, DOWN, ENTER, ESC, DOWN, ENTER, ESC, DOWN, ENTER, ESC, UP, UP, UP, LEFT, RIGHT, RIGHT,
        UP, UP, ENTER, DOWN, ENTER, ESC, DOWN, ENTER, DOWN, ENTER,
    ] {
        r.tap(k);
    }
    r.until("quit", 200);
    r.run(2);
    assert!(r.commands.contains(&Command::Quit));
    let stp = r.storage.read("GAME.STP").expect("GAME.STP written");
    let words: Vec<u32> = stp.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(words, [1, 1, 0, 15, 5, 2, 2, 1]);
    // Menu sounds: 11 for cursor keys, 12 for Enter / Esc.
    assert!(r.game.sfx_log().iter().all(|&(id, v)| (id == 11 || id == 12) && v == 255));
    assert_eq!(r.game.sfx_log().len(), 30);
}

#[test]
fn escape_twice_quits_from_the_main_menu() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ESC);
    assert_eq!(r.game.stage_name(), "menu");
    r.tap(ESC);
    r.until("quit", 100);
}

#[test]
fn cheat_phrase_opens_the_island_list() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.type_text("THE TRUTH IS OUT THERE");
    assert!(r.traced("cheat"));
    r.run(40);
    r.tap(ENTER); // NEW GAME -> CHEAT MODE
    r.tap(DOWN);
    r.tap(DOWN); // YETI ISLAND
    r.tap(ENTER);
    r.until("island", 300);
    assert!(r.traced("island 3"));
    assert_eq!(r.game.world().sim.s.island, 3);
    assert_eq!(r.game.errors(), &[] as &[String]);
}

#[test]
fn new_game_plays_island_one_with_cd_track_five() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    assert!(r.commands.contains(&Command::PlayCdTrack { track: 5, looping: false }));
    // Fade-out of the hidden first frame, two frames, fade-in.
    r.run(100);
    assert!(!r.picture_is_black());
    let before = r.game.world().frames;
    r.run(80);
    assert_eq!(r.game.world().frames - before, 10, "one frame per 8 ticks");
    assert_eq!(r.game.world().sim.s.p.lives, 3);

    // Esc: back to the menu with RESUME GAME and SAVE GAME; saving writes the original's files.
    r.tap(ESC);
    r.until("menu", 50);
    r.run(40);
    for k in [DOWN, DOWN, DOWN, ENTER, ENTER, ENTER] {
        r.tap(k);
    }
    r.tap(ENTER); // "GAME SAVED" message
    let save = r.storage.read("SAVEGAME.__0").expect("slot written");
    assert_eq!(save.len(), 181_817);
    let rec = r.storage.read("SAVEGAME.REC").expect("names written");
    assert_eq!(rec.len(), 80);
    assert_eq!(&rec[..7], b"FREE 1\0");
    // Cancel, RESUME GAME.
    r.tap(ESC);
    r.tap(UP);
    r.tap(ENTER);
    r.until("island", 300);
    assert_eq!(r.game.errors(), &[] as &[String]);
}

#[test]
fn a_save_written_by_the_original_loads() {
    let save = concat!(env!("CARGO_MANIFEST_DIR"), "/../../out/ref/island1_start_SAVEGAME.__0");
    let (Ok(save), Some(_)) = (std::fs::read(save), disc_dir()) else { return };
    let mut storage = SharedStorage::default();
    storage.write("SAVEGAME.__0", &save);
    let mut r = Rig::new(GameOptions { intro: false, credits: false, frame_ticks: 8, mode: Mode::Original }, storage).unwrap();
    r.run(60);
    for k in [DOWN, ENTER, ENTER, DOWN, ENTER] {
        r.tap(k);
    }
    r.until("island", 300);
    assert!(r.traced("load slot 0"));
    let s = &r.game.world().sim.s;
    assert_eq!((s.island, s.p.lives, s.p.px, s.p.pz), (1, 3, -19650, -10575));
    // The difficulty stored in the save replaces the option.
    assert_eq!(r.game.settings().stp.difficulty, 1);
    r.run(200);
    assert_eq!(r.game.errors(), &[] as &[String]);
}

#[test]
fn leaving_island_one_plays_the_ending_and_returns_to_the_menu() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(120);
    r.game.world_mut().sim.s.next_island = 2;
    r.until("ending", 40);
    let start = r.ticks;
    assert!(r.commands.contains(&Command::PlayCdTrack { track: 7, looping: false }));
    r.until("menu", 3000);
    // 33 retraces of fade, PART_1.MOV 15 x 12 frames and PART_2.MOV 71 frames at 7 retraces each.
    let took = r.ticks - start;
    assert!((33 + 180 * 7 + 71 * 7..33 + 180 * 7 + 71 * 7 + 6).contains(&took), "ending took {took} ticks");
    assert!(r.traced("movie PART_1.MOV") && r.traced("movie PART_2.MOV"));
    assert_eq!(r.game.world().sim.s.p.lives, -1);
}

#[test]
fn game_over_and_continue() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(120);
    // No continues: the picture, a key, the menu.
    r.game.world_mut().sim.s.p.lives = -1;
    r.until("game over", 40);
    r.run(60);
    assert!(!r.picture_is_black());
    r.tap(ENTER);
    r.until("menu", 100);
    // With a continue: CONTINUE 9, YES.
    r.run(40);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(120);
    r.game.world_mut().sim.s.p.continues = 1;
    r.game.world_mut().sim.s.p.lives = -1;
    r.until("game over", 40);
    r.run(60);
    r.tap(ENTER);
    r.until("island", 200);
    let p = &r.game.world().sim.s.p;
    assert_eq!((p.lives, p.energy, p.continues), (3, 99, 0));
    // The countdown runs out by itself: 599 passes of 33/8 ticks.
    r.run(120);
    r.game.world_mut().sim.s.p.continues = 1;
    r.game.world_mut().sim.s.p.lives = -1;
    r.until("game over", 40);
    let start = r.ticks;
    r.until("menu", 3000);
    let took = r.ticks - start;
    assert!((2470..2520).contains(&took), "countdown took {took} ticks");
}

#[test]
fn modal_screens_return_to_the_island() {
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(120);
    let events = [
        Event::Say(0),
        Event::Picture("RADEAU.RON"),
        Event::TextOnPicture(1, "RADEAU.RON"),
        Event::Map,
        Event::Inventory,
        Event::Shop,
    ];
    for e in events {
        let before = r.game.world().frames;
        r.game.world_mut().queue_event(e.clone());
        r.run(60);
        assert_eq!(r.game.world().frames, before + 1, "{e:?} holds the game after the frame that raised it");
        assert!(!r.picture_is_black(), "{e:?} is on screen");
        // Space dismisses every one of them (the shop needs Esc).
        r.tap(if e == Event::Shop { ESC } else { 0x39 });
        r.run(120);
        assert!(r.game.world().frames > before + 3, "{e:?} returned to the island");
    }
    // The timed picture needs no key: 2 x (fade 17) + delay 7 + fades back to the game.
    let before = r.game.world().frames;
    r.game.world_mut().queue_event(Event::PictureTimed("EXPLOSE.RON", 16));
    r.run(200);
    assert!(r.game.world().frames > before + 3);
    assert_eq!(r.game.errors(), &[] as &[String]);
    assert_eq!(r.game.stage_name(), "island");
}

// ---- enhanced mode ----------------------------------------------------------

fn enhanced() -> Option<Rig> {
    Rig::new(GameOptions { intro: false, credits: false, frame_ticks: 8, mode: Mode::Enhanced }, SharedStorage::default())
}

impl Rig {
    /// A key the frontends do not bind to a button.
    fn tap_key(&mut self, scancode: u8) {
        self.game.key_event(scancode, true);
        self.run(12);
        self.game.key_event(scancode, false);
        self.run(28);
    }

    fn click(&mut self, x: i32, y: i32) {
        (self.held.mouse_x, self.held.mouse_y) = (x, y);
        self.run(12);
        self.held.mouse_left = true;
        self.run(12);
        self.held.mouse_left = false;
        self.run(28);
    }

    fn picture(&mut self) -> Vec<u8> {
        let mut fb = Framebuffer::new();
        self.game.render(&mut fb);
        fb.pixels.to_vec()
    }
}

const KEY_W: u8 = 0x11;
const KEY_S: u8 = 0x1f;

#[test]
fn enhanced_menus_take_wasd_and_space() {
    // W from NEW GAME wraps to EXIT, Space opens EXIT GAME ?, S moves to YES, Space quits.
    let Some(mut r) = enhanced() else { return };
    r.run(60);
    for k in [KEY_W, 0x39, KEY_S, 0x39] {
        r.tap_key(k);
    }
    r.until("quit", 200);
    // The original reads the same keys as text.
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    for k in [KEY_W, 0x39, KEY_S, 0x39] {
        r.tap_key(k);
    }
    r.run(200);
    assert_eq!(r.game.stage_name(), "menu");
}

#[test]
fn enhanced_menus_take_the_mouse() {
    let Some(mut r) = enhanced() else { return };
    r.run(60);
    r.click(320, 332); // EXIT
    assert_eq!(r.game.stage_name(), "menu");
    r.click(320, 292); // YES, SORRY...
    r.until("quit", 200);

    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.click(320, 332);
    r.click(320, 292);
    r.run(200);
    assert_eq!(r.game.stage_name(), "menu");
}

#[test]
fn enhanced_level_select_phrase_still_works() {
    let Some(mut r) = enhanced() else { return };
    r.run(60);
    r.type_text("THE TRUTH IS OUT THERE");
    assert!(r.traced("cheat"));
    assert_eq!(r.game.stage_name(), "menu", "the spaces of the phrase chose nothing");
    r.run(40);
    r.tap(ENTER);
    r.tap(DOWN);
    r.tap(ENTER);
    r.until("island", 300);
    assert!(r.traced("island 2"));
}

#[test]
fn enhanced_escape_in_the_main_menu_resumes() {
    let Some(mut r) = enhanced() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(120);
    r.tap(ESC);
    r.until("menu", 50);
    r.run(40);
    r.tap(ESC);
    r.until("island", 300);
    r.run(200);
    assert_eq!(r.game.stage_name(), "island");
    assert!(r.game.world().frames > 5);
    assert_eq!(r.game.errors(), &[] as &[String]);
}

#[test]
fn enhanced_view_is_drawn_between_simulation_frames() {
    let Some(mut r) = enhanced() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(150);
    r.game.key_event(UP, true);
    r.held.up = true;
    r.run(80);
    // Line up with the start of a simulation frame, then look twice inside it.
    let f = r.game.world().frames;
    while r.game.world().frames == f {
        r.run(1);
    }
    let frames = r.game.world().frames;
    let a = r.picture();
    r.run(3);
    let b = r.picture();
    assert_eq!(r.game.world().frames, frames, "no simulation frame in between");
    let changed = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert!(changed > 1000, "the view moved between two simulation frames ({changed} pixels)");

    // The minimap: a frame of one colour around 96 x 96 pixels at (520, 64) of the view.
    let at = |x: usize, y: usize| b[(40 + y) * 640 + x];
    let frame = at(519, 63);
    assert!((519..=616).all(|x| at(x, 63) == frame && at(x, 160) == frame));
    assert!((63..=160).all(|y| at(519, y) == frame && at(616, y) == frame));

    // The original draws one picture per simulation frame and no map.
    let Some(mut r) = Rig::plain() else { return };
    r.run(60);
    r.tap(ENTER);
    r.until("island", 300);
    r.run(150);
    r.game.key_event(UP, true);
    r.held.up = true;
    r.run(80);
    let f = r.game.world().frames;
    while r.game.world().frames == f {
        r.run(1);
    }
    let a = r.picture();
    r.run(3);
    assert_eq!(a, r.picture());
}

#[test]
fn safe_spawn_keeps_the_start_of_island_one_quiet() {
    use pd_game::settings::Enhancements;
    let run = |enh: Enhancements| -> Option<(i32, i32)> {
        let dir = disc_dir()?;
        let storage = SharedStorage::default();
        let opts = GameOptions { intro: false, credits: false, frame_ticks: 4, mode: Mode::Enhanced };
        let game = Game::with_enhancements(Box::new(DirAssets(dir)), Box::new(storage.clone()), opts, enh);
        let mut r = Rig { game, storage, commands: Vec::new(), ticks: 0, held: Input::default() };
        r.run(60);
        r.tap(ENTER);
        r.until("island", 300);
        // Standing still: four seconds, then another ten.
        r.run(280);
        let early = r.game.world().sim.s.p.energy;
        r.run(700);
        Some((early, r.game.world().sim.s.p.energy))
    };
    let Some((early, _)) = run(Enhancements::enhanced()) else { return };
    assert_eq!(early, 99, "nothing has hurt the duck yet");
    let (early, late) = run(Enhancements { safe_spawn: false, ..Enhancements::enhanced() }).unwrap();
    eprintln!("without safe_spawn: energy {early} after 4 s, {late} after 14 s");
    assert!(late < 99, "the original start is under fire");
}

#[test]
fn enhanced_dialogue_waits_for_a_confirm_key() {
    for enhanced_mode in [true, false] {
        let Some(mut r) = (if enhanced_mode { enhanced() } else { Rig::plain() }) else { return };
        r.run(60);
        r.tap(ENTER);
        r.until("island", 300);
        r.run(400);
        let before = r.game.world().frames;
        r.game.world_mut().queue_event(Event::Say(0));
        r.run(60);
        // Walking on (W / Up) while the text is up.
        r.held.up = true;
        r.tap(UP);
        r.held.up = false;
        r.run(120);
        let dismissed = r.game.world().frames > before + 3;
        assert_eq!(dismissed, !enhanced_mode, "a movement key ends the dialogue only in the original");
        if enhanced_mode {
            r.tap(0x39);
            r.run(120);
            assert!(r.game.world().frames > before + 3, "Space ends it");
        }
    }
}
