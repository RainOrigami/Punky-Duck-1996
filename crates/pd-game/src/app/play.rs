//! Island play: the glue between the simulation ([`crate::sim`]), the island
//! renderer ([`crate::render`]) and the program flow. Everything that touches
//! those two modules is in this file.
//!
//! One rendered frame of the original (`main` loop body, `0x12444`):
//! read input, run `elapsed ticks + speed + 1` simulation steps, per-frame
//! logic, modal screens (dialogue, pictures, map, inventory, shop), render,
//! HUD, present. The original renders as fast as the machine allows; here a
//! frame takes [`World::frame_ticks`] 70 Hz ticks (logic at the start,
//! picture on screen at the end).

use std::collections::VecDeque;

use pd_engine::Input;
use pd_formats::trj::Trj;
use pd_formats::txt::TextFile;

use super::core::{Core, Op, Picture};
use super::gfx::{self, Align, BLACK, Canvas, FontState, Pal, Sheet};

use super::keys::sc;
use super::menu::SaveGame;
use super::store::MOUSE_SPEED_DEFAULT;
use crate::render::{
    Anchor, Camera, IslandAssets, ObjectInstance, Renderer, Scene, SpriteBank, ViewMode, ViewState, WorldSprite,
};
use crate::settings::Enhancements;
use crate::sim::shop::{BuyResult, PRODUCT_TABLE, Shop};
use crate::sim::{Bank, Event, InputEdges, ModernInput, Sim, SimInput, SpriteDraw, Status};

/// Why the frame loop of `main` ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// Esc is down (`g_keys[1]`).
    Escape,
    GameOver,
    NextIsland(i32),
    Ending,
}

/// The shop screen (`0x2a450`): a hand cursor moved with the arrow keys,
/// Space "clicks" one of 22 hot zones.
struct ShopUi {
    shop: Shop,
    x: i32,
    y: i32,
    sel: i32,
    pal: Pal,
    /// Picture as loaded, to restore the areas drawn over each pass.
    clean: Vec<u8>,
    leaving: bool,
    /// Pointer position at the previous pass (`Enhancements::menu_mouse`).
    mouse: (i32, i32),
    /// Where the hand was drawn last, to take it off the screen after a jump.
    drawn: (i32, i32),
}

/// Hot zones of the shop (`0x2b3a0` calls at `0x2a52b`): BUY, CANCEL, EXIT,
/// then the 19 products.
fn shop_zones() -> Vec<[i32; 4]> {
    let mut z = vec![[0x1c, 0x112, 0x7f, 0x126], [0x1c, 0x138, 0x7f, 0x14b], [0xe, 0x1b5, 0x5d, 0x1cb]];
    z.extend((0..8).map(|k| [k * 0x48 + 0x11, 0x14, k * 0x48 + 0x56, 0x59]));
    z.extend((0..7).map(|k| [k * 0x48 + 0x11, 0x69, k * 0x48 + 0x56, 0xae]));
    z.extend((0..4).map(|k| [k * 0x48 + 0xa1, 0xc0, k * 0x48 + 0xe6, 0x105]));
    z
}

enum Modal {
    None,
    /// Queued ops are running; `finish` runs when they are done.
    Ops,
    Shop(Box<ShopUi>),
}

pub struct World {
    pub sim: Sim,
    enh: Enhancements,
    renderer: Renderer,
    island: Option<IslandAssets>,
    /// Island whose `ILE_n.UNI` is in memory (`0x40014` flag; 0 = none).
    uni_loaded: i32,
    duck: SpriteBank,
    boom: SpriteBank,
    enemy: SpriteBank,
    boss: SpriteBank,
    bonus: SpriteBank,
    icone: SpriteBank,
    /// `NUMBERS.SPT` (`0x1695d0`), `ICONE_n.SPT`, `BONUS_n.SPT`, `HAND.SPT` for the 2D screens.
    numbers: Sheet,
    icone_sheet: Sheet,
    bonus_sheet: Sheet,
    hand: Sheet,
    /// `ANIMAPS.RON` (`0x21c5c8`, pitch 192, palette `0x489690`).
    animaps: Option<Picture>,
    text: Option<TextFile>,
    view: ViewState,
    /// `0x40058`: 1 = view needs its mode switch, 3 and 2 on the way to the fade-in.
    reinit: u8,
    /// `g_ticks` (`0x41b02`): 70 Hz ticks since the simulation last consumed them.
    g_ticks: u32,
    /// Frame counter of `cd_loop_check` (`0x40020`).
    frame_no: u32,
    /// Ticks one rendered frame takes.
    pub frame_ticks: u32,
    /// Ticks until the frame being rendered is presented; 0 = start a new frame.
    wait: u32,
    edges: InputEdges,
    /// Number keys held on the previous frame, bit n = scancode `0x02 + n` (modern controls).
    digits_prev: u16,
    events: VecDeque<Event>,
    modal: Modal,
    /// In-game cheat latches (`0x4191a`).
    cheats: u16,
    /// Paused by P / Pause or the mouse button: 1 = waiting for release, 2 = for the next press.
    pause: u8,
    /// Clear `g_ticks` when the queued fade has finished (`0x149d7`).
    zero_ticks: bool,
    /// Slot file just read by LOAD GAME, until `prepare_loaded` ran.
    pub pending_load: Option<Vec<u8>>,
    /// Frames presented since the island was entered (tests, traces).
    pub frames: u64,
    /// Esc is ignored until it has been seen up (`Enhancements::menu_esc_resumes`:
    /// the key that left the menu may still be down).
    esc_block: bool,
    /// The last two simulation frames as drawn (`Enhancements::smooth_view`).
    snap: Option<Snap>,
    snap_prev: Option<Snap>,
    minimap: Option<MiniMap>,
    /// Heading change of the last simulation frame without the mouse's part.
    key_turn: f32,
}

/// What one simulation frame handed to the renderer.
struct Snap {
    cam_x: i32,
    cam_z: i32,
    angle: f32,
    /// Part of the heading change of this frame that came from the keys.
    key_turn: f32,
    view: ViewState,
    objects: Vec<ObjectInstance>,
    sprites: Vec<SpriteDraw>,
    hud: HudState,
}

/// `CARTE0n.RON` at one pixel per cell in the island's colours
/// (`Enhancements::minimap`).
struct MiniMap {
    /// 256 x 240, row = map row (cell z - 8).
    cells: Vec<u8>,
    dark: u8,
    frame: u8,
    tele: u8,
    hint: u8,
    duck: u8,
}

fn nearest(pal: &Pal, rgb: [i32; 3]) -> u8 {
    (0..256usize)
        .min_by_key(|&i| (0..3).map(|k| (pal[i][k] as i32 - rgb[k]).pow(2)).sum::<i32>())
        .unwrap_or(0) as u8
}

fn lerp(a: i32, b: i32, t: f32) -> i32 {
    a + ((b - a) as f32 * t).round() as i32
}

/// Largest jump in world units that is still shown as movement.
const LERP_MAX: i32 = 600;

fn lerp_near(a: i32, b: i32, t: f32) -> i32 {
    if (b - a).abs() > LERP_MAX { b } else { lerp(a, b, t) }
}

fn empty_bank() -> SpriteBank {
    SpriteBank { sprites: Vec::new() }
}

/// Heading change per device pixel of mouse movement (modern controls).
const MOUSE_SENSITIVITY: f32 = 0.0025;

fn sim_input(i: &Input) -> SimInput {
    SimInput {
        up: i.up,
        down: i.down,
        left: i.left,
        right: i.right,
        strafe: i.action,
        jetpack: i.item,
        next_weapon: i.next_item,
        view: i.view,
        interlace: i.interlace,
        fire: i.secondary,
        map: i.inventory,
        inventory: i.misc,
        esc: i.cancel,
        key1: i.key1,
        key2: i.key2,
    }
}

impl SaveGame for World {
    fn island(&self) -> i32 {
        self.sim.s.island
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(self.sim.s.write_save())
    }

    fn load(&mut self, data: &[u8]) -> bool {
        // Only the state is read here (`0x293c0`); `prepare_loaded` rebuilds
        // the rest once the island files are in memory.
        let ok = self.sim.s.read_save(data).is_ok();
        if ok {
            self.pending_load = Some(data.to_vec());
        }
        ok
    }
}

impl World {
    pub fn new(enh: Enhancements, frame_ticks: u32) -> Self {
        Self {
            sim: Sim::new(enh.clone()),
            enh,
            renderer: Renderer::new(),
            island: None,
            uni_loaded: 0,
            duck: empty_bank(),
            boom: empty_bank(),
            enemy: empty_bank(),
            boss: empty_bank(),
            bonus: empty_bank(),
            icone: empty_bank(),
            numbers: Sheet::empty(),
            icone_sheet: Sheet::empty(),
            bonus_sheet: Sheet::empty(),
            hand: Sheet::empty(),
            animaps: None,
            text: None,
            view: ViewState::default(),
            reinit: 1,
            g_ticks: 0,
            frame_no: 0,
            frame_ticks: frame_ticks.max(1),
            wait: 0,
            edges: InputEdges::default(),
            digits_prev: 0,
            events: VecDeque::new(),
            modal: Modal::None,
            cheats: 0,
            pause: 0,
            zero_ticks: false,
            pending_load: None,
            frames: 0,
            esc_block: false,
            snap: None,
            snap_prev: None,
            minimap: None,
            key_turn: 0.0,
        }
    }

    /// Radians per device pixel at the MOUSE SPEED setting.
    fn mouse_scale(core: &Core) -> f32 {
        MOUSE_SENSITIVITY * 1.25f32.powi(core.settings.mouse_speed as i32 - MOUSE_SPEED_DEFAULT as i32)
    }

    fn load_minimap(&self, core: &mut Core, n: i32) -> Option<MiniMap> {
        let pic = core.load_picture(&format!("CARTE0{n}.RON"))?;
        let pal = self.island_pal();
        let remap: Vec<u8> = (0..256).map(|i| nearest(&pal, pic.pal[i].map(i32::from))).collect();
        let mut cells = vec![0u8; 256 * 240];
        for (cz, row) in cells.chunks_exact_mut(256).enumerate() {
            for (cx, c) in row.iter_mut().enumerate() {
                if let Some(&p) = pic.pixels.get(cz * 2 * pic.w + cx * 2) {
                    *c = remap[p as usize];
                }
            }
        }
        Some(MiniMap {
            cells,
            dark: nearest(&pal, [4, 4, 6]),
            frame: nearest(&pal, [50, 50, 50]),
            tele: nearest(&pal, [16, 32, 63]),
            hint: nearest(&pal, [0, 63, 0]),
            duck: nearest(&pal, [63, 8, 8]),
        })
    }

    fn bank(core: &mut Core, name: &str) -> SpriteBank {
        match core.banque(name).map(SpriteBank::parse) {
            Some(Ok(b)) => b,
            _ => {
                core.errors.push(format!("{name}: not in BANQUE.BNK"));
                empty_bank()
            }
        }
    }

    /// `uni_init` plus the files the renderer needs (`0x120d0`, `0x12212`).
    fn load_island_files(&mut self, core: &mut Core, n: i32) -> bool {
        if self.uni_loaded == n && self.island.is_some() {
            return true;
        }
        let uni = core.assets.load(&format!("ILE_{n}.UNI"));
        let col = core.assets.load(&format!("ILE_{n}.COL"));
        let d3 = core.assets.load(&format!("OBJETS_{n}.3D"));
        let map = core.assets.load(&format!("OBJETS_{n}.MAP"));
        let ciel = core.load_picture_raw(&format!("CIEL_{n}.RON"));
        let (Some(uni), Some(col)) = (uni, col) else {
            core.errors.push(format!("ILE_{n}.UNI / ILE_{n}.COL: not found"));
            return false;
        };
        let objects = match (&d3, &map) {
            (Some(a), Some(b)) => Some((a.as_slice(), b.as_slice())),
            _ => None,
        };
        match IslandAssets::from_bytes(&uni, &col, objects, ciel.as_deref()) {
            Ok(a) => {
                self.island = Some(a);
                self.uni_loaded = n;
                true
            }
            Err(e) => {
                core.errors.push(format!("island {n}: {e}"));
                false
            }
        }
    }

    fn trj(core: &mut Core, name: &str) -> Trj {
        match core.assets.load(name).and_then(|d| Trj::parse(&d).ok()) {
            Some(t) => t,
            None => {
                core.errors.push(format!("{name}: cannot load"));
                Trj { paths: Vec::new(), records: Vec::new(), points: Vec::new() }
            }
        }
    }

    /// NEW GAME (`game_reset(0)`).
    pub fn new_game(&mut self, core: &Core) {
        self.sim.s.difficulty = core.settings.stp.difficulty as i32;
        self.sim.new_game();
    }

    /// New game or next island: the "not resumed" branch of `main` (`0x120b0`).
    pub fn start_island(&mut self, core: &mut Core, n: i32) -> bool {
        self.uni_loaded = 0;
        if !self.load_island_files(core, n) {
            return false;
        }
        self.sim.s.difficulty = core.settings.stp.difficulty as i32;
        let bonus = Self::trj(core, &format!("ILE0{n}.TRJ"));
        let monsters = Self::trj(core, &format!("MONSTER{n}.TRJ"));
        let island = self.island.as_ref().expect("loaded above");
        self.sim.start_island(n, &island.uni, &bonus, monsters);
        true
    }

    /// After LOAD GAME (`0x264f0`): `trj_load_monsters` and the pointer
    /// rebuild, with the island of the save.
    pub fn prepare_loaded(&mut self, core: &mut Core, slot_data: &[u8]) -> bool {
        let n = self.sim.s.island;
        if !(1..=5).contains(&n) {
            return false;
        }
        self.uni_loaded = 0;
        if !self.load_island_files(core, n) {
            return false;
        }
        let monsters = Self::trj(core, &format!("MONSTER{n}.TRJ"));
        let island = self.island.as_ref().expect("loaded above");
        self.sim.load_save(slot_data, &island.uni, monsters).is_ok()
    }

    /// `island_enter` and the set-up before the frame loop (`0x121db`).
    pub fn enter(&mut self, core: &mut Core) -> bool {
        let n = self.sim.s.island;
        if !self.load_island_files(core, n) {
            return false;
        }
        core.note(format!("island {n}"));
        // bios_set_mode(0x13): the screen goes blank.
        core.video.fill_screen(0);
        // Island asset function (0x174c0 and siblings).
        core.coolfnt1 = core.load_sheet("COOLFNT1.SPT");
        core.font = FontState::set(&core.coolfnt1, 0x20, -5);
        self.duck = Self::bank(core, "DUCK.SPT");
        self.enemy = Self::bank(core, &format!("ENEMY{n}.SPT"));
        self.icone = Self::bank(core, &format!("ICONE_{n}.SPT"));
        self.bonus = Self::bank(core, &format!("BONUS_{n}.SPT"));
        self.boom = Self::bank(core, "BOOM.SPT");
        self.boss = Self::bank(core, &format!("BOSS{n}.SPT"));
        self.numbers = core.load_sheet("NUMBERS.SPT");
        self.icone_sheet = core.load_sheet(&format!("ICONE_{n}.SPT"));
        self.bonus_sheet = core.load_sheet(&format!("BONUS_{n}.SPT"));
        self.animaps = core.load_picture("ANIMAPS.RON");
        self.text = core.assets.load(&format!("ILE_{n}.TXT")).and_then(|d| TextFile::parse(&d).ok());
        self.sim.enter();
        self.minimap = if self.enh.minimap { self.load_minimap(core, n) } else { None };
        self.snap = None;
        self.snap_prev = None;
        self.esc_block = self.enh.menu_esc_resumes;
        core.clicks = (false, false);
        core.mouse_dx = 0.0;

        core.video.set_palette(&BLACK);
        self.reinit = 1;
        self.view = ViewState::default();
        self.renderer.reset();
        if self.enh.far_view {
            self.renderer.set_reach(crate::render::Reach::cells(crate::settings::FAR_VIEW_CELLS));
        }
        core.kbd.set_bios(false);
        self.edges = InputEdges::default();
        self.edges.poll(&sim_input(&core.input));
        core.sound.play_loop(super::sound::SFX_SEA, 0);
        core.cd_stop();
        if core.settings.stp.music_on != 0 {
            core.cd_play(5);
        }
        self.wait = 0;
        // Ticks the timer counted since the menu last cleared it: its
        // fade-out and the loading from disc. Machine dependent; this is the
        // DOSBox-X reference set-up (17 + 60 retraces).
        self.g_ticks = 77;
        self.events.clear();
        self.modal = Modal::None;
        self.pause = 0;
        self.frames = 0;
        true
    }

    /// Queue an event as if the simulation had raised it (tests, debugging).
    pub fn queue_event(&mut self, e: Event) {
        self.events.push_back(e);
    }

    /// Leaving the frame loop: `demo_close`, `uni_loaded = 0` (`0x124fd`).
    pub fn left(&mut self) {
        self.uni_loaded = 0;
    }

    pub fn reset_ticks(&mut self) {
        self.g_ticks = 0;
    }

    fn island_pal(&self) -> Pal {
        self.island.as_ref().map_or(BLACK, |i| i.palette)
    }

    /// Palette of `ANIMAPS.RON`, which the ending fades out from (`0x12531`).
    pub fn animaps_pal(&self) -> Pal {
        self.animaps.as_ref().map_or(BLACK, |p| p.pal)
    }

    fn view_mode(core: &Core) -> ViewMode {
        match core.settings.view_mode {
            0 => ViewMode::Low13h,
            1 => ViewMode::LowWindow,
            _ => ViewMode::High,
        }
    }

    // ---- in-game cheats (`input_poll`, `0x26ec0`) --------------------------

    fn poll_cheats(&mut self, core: &mut Core) {
        let k = &core.kbd;
        let mut flash = false;
        if self.cheats & 0x8000 == 0 {
            if k.exactly([0x23, 0x2e, 0x14]) {
                self.cheats |= 0x8000;
                flash = true;
            }
        } else {
            let combos: [([u8; 3], u16); 7] = [
                ([0x2e, 0x13, 0x21], 0x100),
                ([0x30, 0x16, 0x18], 0x80),
                ([0x26, 0x21, 0x17], 0x1),
                ([0x31, 0x13, 0x24], 0x2),
                ([0x27, 0x10, 0x18], 0x4),
                ([0x24, 0x12, 0x14], 0x8),
                ([0x13, 0x21, 0x1e], 0x10),
            ];
            for (keys, bit) in combos {
                if k.exactly(keys) && self.cheats & bit == 0 {
                    self.cheats |= bit;
                    flash = true;
                }
            }
        }
        if flash {
            core.note(format!("cheat flags {:#06x}", self.cheats));
            let pal = self.island_pal();
            let red = gfx::red_palette(&pal);
            core.fade(4, &pal, &red);
            core.fade(0x10, &red, &pal);
        }
        self.sim.apply_cheats(self.cheats & 0x7fff);
    }

    /// `true` while the pointer belongs to the island view: always, except in the
    /// shop, whose hand it moves.
    pub fn wants_mouse(&self) -> bool {
        self.enh.modern_controls && !matches!(self.modal, Modal::Shop(_))
    }

    /// `true` while mouse movement turns the duck.
    pub fn wants_turn(&self) -> bool {
        self.enh.modern_controls && matches!(self.modal, Modal::None) && self.pause == 0 && self.sim.s.p.dead == 0
    }

    // ---- frame loop --------------------------------------------------------

    /// One 70 Hz tick of the island loop. `Some` when the loop of `main` ends.
    pub fn tick(&mut self, core: &mut Core) -> Option<Leave> {
        self.g_ticks = self.g_ticks.saturating_add(1);
        if core.busy() {
            return None;
        }
        if self.zero_ticks {
            self.zero_ticks = false;
            self.g_ticks = 0;
        }
        match std::mem::replace(&mut self.modal, Modal::None) {
            Modal::None => {}
            Modal::Ops => {
                // The modal screen is over: back to the island view.
                self.g_ticks = 0;
                self.reinit = 1;
                if !self.next_modal(core) {
                    self.render(core);
                }
                return None;
            }
            Modal::Shop(mut ui) => {
                if self.shop_tick(core, &mut ui) {
                    self.modal = Modal::Ops;
                } else {
                    self.modal = Modal::Shop(ui);
                }
                return None;
            }
        }
        if self.wait > 0 {
            self.wait -= 1;
            if self.wait == 0 {
                self.present(core);
            }
            if self.wait > 0 || core.busy() {
                return None;
            }
        }

        // Top of the loop: `while (!key[ESC] && lives >= 0 && next_island == 0)`.
        if core.kbd.is_down(sc::ESC) || core.input.cancel {
            if !self.esc_block {
                return Some(Leave::Escape);
            }
        } else {
            self.esc_block = false;
        }
        match self.sim.status() {
            Status::Playing => {}
            Status::GameOver => return Some(Leave::GameOver),
            Status::NextIsland(n) => return Some(Leave::NextIsland(n)),
            Status::Ending => return Some(Leave::Ending),
        }
        // Pause: the mouse button (0x12450) and P / Pause (0x27240).
        let p_key = core.input.pause;
        match self.pause {
            0 if core.input.mouse_left && !self.enh.modern_controls => {
                self.pause = 3;
                return None;
            }
            3 => {
                if core.input.mouse_left {
                    return None;
                }
                self.pause = 0;
            }
            0 if p_key => {
                self.pause = 1;
                return None;
            }
            1 => {
                if !p_key {
                    self.pause = 2;
                }
                return None;
            }
            2 => {
                if !p_key {
                    return None;
                }
                // The key that ends the pause is still down on the next
                // poll, which pauses again until it is released and pressed
                // once more; wait for its release here instead.
                self.pause = 4;
                return None;
            }
            4 => {
                if p_key {
                    return None;
                }
                self.pause = 0;
            }
            _ => {}
        }

        // game_tick_frame (0x2a260).
        self.poll_cheats(core);
        let mut held = sim_input(&core.input);
        let mut turn = 0.0;
        if self.enh.modern_controls {
            let k = &core.kbd;
            let (a, d) = (k.is_down(0x1e), k.is_down(0x20));
            // A and D reach `Input` as left/right; they strafe here, the arrows still turn.
            if a && !k.is_down(sc::LEFT) {
                held.left = false;
            }
            if d && !k.is_down(sc::RIGHT) {
                held.right = false;
            }
            held.fire |= core.input.mouse_left;
            held.jetpack |= k.is_down(0x12);
            turn = core.mouse_dx * Self::mouse_scale(core);
            // The number row selects weapons, so the quest pictures move to F3 / F4.
            held.key1 = k.is_down(0x3d);
            held.key2 = k.is_down(0x3e);
            let digits = (0..10).fold(0u16, |m, n| m | (k.is_down(0x02 + n) as u16) << n);
            let pressed = digits & !self.digits_prev;
            self.digits_prev = digits;
            self.sim.modern = ModernInput {
                turn,
                strafe_left: a,
                strafe_right: d,
                select_weapon: (pressed != 0).then(|| pressed.trailing_zeros() as u8),
            };
        }
        core.mouse_dx = 0.0;
        let input = self.edges.poll(&held);
        let ticks = self.g_ticks as i32 + core.settings.stp.game_speed;
        self.g_ticks = 0;
        let before = self.sim.s.p.angle;
        if self.sim.s.p.dead != 0 {
            turn = 0.0;
        }
        self.sim.frame(&input, ticks);
        // The heading is kept within one turn; anything large is a jump, not a turn.
        let d = (self.sim.s.p.angle - before - turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.key_turn = if d.abs() < 0.5 { d } else { 0.0 };
        self.frame_no = self.frame_no.wrapping_add(1);
        // cd_loop_check (0x11f70).
        if core.settings.stp.music_on != 0 && self.frame_no % 10 == 0 && !core.cd_playing {
            core.cd_stop();
            core.cd_play(5);
        }
        self.events.extend(self.sim.drain_events());
        if !self.next_modal(core) {
            self.render(core);
        }
        None
    }

    /// Handle events up to and including the next modal one. `true` if a
    /// modal screen was started.
    fn next_modal(&mut self, core: &mut Core) -> bool {
        while let Some(e) = self.events.pop_front() {
            match e {
                Event::Sfx { sample, volume } => core.sound.play(sample.max(0) as usize, volume.clamp(0, 255) as u8),
                Event::SeaVolume(v) => core.sound.set_loop_volume(v.clamp(0, 255) as u8),
                Event::CdTrack(t) => {
                    if core.settings.stp.music_on != 0 {
                        core.cd_stop();
                        core.cd_play(t as u8);
                    }
                }
                Event::FadeIn => self.reinit = 1,
                Event::LoadBossBank => {
                    let n = self.sim.s.island;
                    self.boss = Self::bank(core, &format!("BOSS{n}.SPT"));
                }
                Event::LoadDuckBank(name) => self.duck = Self::bank(core, name),
                Event::ViewCycle => {
                    // 0x11960: F1.
                    core.settings.view_mode = (core.settings.view_mode + 1) % 3;
                    if core.settings.view_mode != 2 {
                        core.settings.fast_mode = 0;
                    }
                    self.reinit = 1;
                }
                Event::InterlaceToggle => {
                    if core.settings.view_mode == 2 {
                        core.settings.fast_mode ^= 1;
                        core.sound.play(super::sound::SFX_SELECT, 0xff);
                    }
                }
                Event::Say(id) => {
                    let n = self.sim.s.island;
                    let name = if n == 5 && self.sim.s.q.outfit_on != 0 {
                        "DIAL_51.RON".to_string()
                    } else {
                        format!("DIAL_{n}.RON")
                    };
                    self.picture(core, &name, 10, Some(id), false, Wait::Input);
                    return true;
                }
                Event::Picture(name) => {
                    self.picture(core, name, 10, None, true, Wait::Input);
                    return true;
                }
                Event::PictureTimed(name, steps) => {
                    self.picture(core, name, steps, None, true, Wait::Delay);
                    return true;
                }
                Event::TextOnPicture(id, name) => {
                    self.picture(core, name, 10, Some(id), false, Wait::Input);
                    return true;
                }
                Event::Inventory => {
                    self.inventory(core);
                    return true;
                }
                Event::Map => {
                    self.map(core);
                    return true;
                }
                Event::Shop => {
                    self.shop_open(core);
                    return true;
                }
            }
        }
        false
    }

    /// The four dialogue strings of `id` in the selected language (`0x25140`).
    fn lines(&self, core: &Core, id: i32) -> [Vec<u8>; 4] {
        let lang = (core.settings.stp.language as usize).min(4);
        self.text
            .as_ref()
            .and_then(|t| t.lookup(id.max(0) as u32))
            .map(|e| e.raw[lang].clone())
            .unwrap_or_default()
    }

    /// `picture` (`0x28750`), `picture_fade` (`0x28870`), `picture_timed`
    /// (`0x28990`), `say` (`0x28e00`) and `text_on_picture` (`0x290e0`).
    fn picture(&mut self, core: &mut Core, name: &str, steps: i32, text: Option<i32>, poll_first: bool, wait: Wait) {
        self.modal = Modal::Ops;
        let _ = poll_first; // input_poll(0): no state of its own here
        let from = self.island_pal();
        let lines = text.map(|id| self.lines(core, id));
        queue_picture(core, name.to_string(), steps, from, lines, wait);
    }

    /// Between two islands: `picture_fade("TRANSa-b.RON", 150)` (`0x12601`).
    pub fn transition_picture(&mut self, core: &mut Core, next: i32) {
        let from = self.island_pal();
        queue_picture(core, format!("TRANS{}-{}.RON", next - 1, next), 0x96, from, None, Wait::Input);
    }

    /// The inventory screen (`0x185a0`): `INV_0n.RON`, the icons of the items
    /// carried in a 4 x 4 grid; Space leaves.
    fn inventory(&mut self, core: &mut Core) {
        self.modal = Modal::Ops;
        let n = self.sim.s.island;
        let from = self.island_pal();
        let inv: Vec<i32> = self.sim.s.q.inv[..(self.sim.s.q.ninv.clamp(0, 20) as usize)].to_vec();
        core.fade(10, &from, &BLACK);
        core.then(move |core| {
            core.video.fill_screen(0);
            core.video.clip_y0 = 0;
            core.video.clip_y1 = 0x1df;
            core.video.set_palette(&BLACK);
            let pic = core.load_picture(&format!("INV_0{n}.RON"));
            let pal = pic.as_ref().map_or(BLACK, |p| p.pal);
            if let Some(p) = &pic {
                core.video.blit_picture(&p.pixels, p.w, p.h);
            }
            let icons = core.load_sheet(&format!("ICONE_{n}.SPT"));
            for (i, id) in inv.iter().enumerate() {
                let i = i as i32;
                core.video.sprite(&icons, (*id + 8).max(0) as usize, (i % 4) * 100 + 0x46, (i / 4) * 100 + 0x5a);
            }
            for i in 0..16 {
                let (x, y) = ((i % 4) * 100, (i / 4) * 100);
                core.video.canvas().rect_outline(x + 0x1e, y + 0x32, x + 0x6e, y + 0x82, 0xfe);
            }
            core.video.present_size(0x27f, 0x1df);
            let mut tail = vec![
                Op::Fade(gfx::Fade::new(0, 10, &BLACK, &pal)),
                // while (!input.space) input_poll(1);
                Op::Until(Box::new(|core: &Core| core.input.secondary || (core.enh.menu_mouse && core.input.mouse_left))),
                Op::Fade(gfx::Fade::new(0, 10, &pal, &BLACK)),
                Op::Do(Box::new(|core: &mut Core| core.video.fill_screen(0xfe))),
            ];
            while let Some(op) = tail.pop() {
                core.ops.push_front(op);
            }
        });
    }

    /// The map screen (`map_screen`, `0x280f0`).
    fn map(&mut self, core: &mut Core) {
        self.modal = Modal::Ops;
        let s = &self.sim.s;
        let n = s.island;
        let from = self.island_pal();
        let explored = s.explored.clone();
        let tele: Vec<(i32, i32)> = (0..10).map(|i| (s.q.tele_x[i], s.q.tele_z[i])).collect();
        let hint: Vec<(i32, i32)> = (0..20).map(|i| (s.q.hint_x[i], s.q.hint_z[i])).collect();
        let (rx, rz, angle, compass) = (s.p.rx, s.p.rz, s.p.angle as f64, s.p.compass);
        core.fade(10, &from, &BLACK);
        core.then(move |core| {
            core.video.fill_screen(0);
            core.video.clip_y0 = 0;
            core.video.clip_y1 = 0x1df;
            core.video.set_palette(&BLACK);
            // Unexplored picture straight to the screen, explored one to the back buffer.
            if let Some(p) = core.load_picture(&format!("CARTE1{n}.RON")) {
                core.video.blit_picture(&p.pixels, p.w, p.h);
            }
            core.video.present_size(0x27f, 0x1df);
            let pic = core.load_picture(&format!("CARTE0{n}.RON"));
            let mut pal = pic.as_ref().map_or(BLACK, |p| p.pal);
            if let Some(p) = &pic {
                core.video.blit_picture(&p.pixels, p.w, p.h);
            }
            pal[0xfd] = [0, 0x3f, 0];
            pal[0xfe] = [0, 0, 0x3f];
            pal[0xff] = [0x3f, 0, 0];
            let v = &mut core.video;
            let put = |v: &mut gfx::Video, x: i32, y: i32, c: u8| {
                if (0..gfx::W as i32).contains(&x) && (0..gfx::H as i32).contains(&y) {
                    v.screen.pixels[y as usize * gfx::W + x as usize] = c;
                }
            };
            for cz in 0..0xf0usize {
                for cx in 0..0x100usize {
                    // The reader's base is 0x2456b0, 8 bytes into the saved array (0x2456a8).
                    if explored.get(cx * 256 + cz + 8).is_some_and(|&e| e != 0) {
                        for (dx, dy) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                            let (x, y) = (cx * 2 + dx, cz * 2 + dy);
                            let c = v.back[y * gfx::W + x];
                            v.screen.pixels[y * gfx::W + x] = c;
                        }
                    }
                }
            }
            let block = |v: &mut gfx::Video, x0: i32, y0: i32, x1: i32, y1: i32, c: u8| {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        put(v, x, y, c);
                    }
                }
            };
            for (list, colour) in [(&tele, 0xfe), (&hint, 0xfd)] {
                for &(x, z) in list.iter() {
                    if x != -1 {
                        let (mx, mz) = (x * 2 / 300, z * 2 / 300);
                        block(v, mx + 0xff, mz + 0xf3, mx + 0x101, mz + 0xf5, colour);
                    }
                }
            }
            let sx = -(rx * 2 / 300) + 0x100;
            let sy = -(rz * 2 / 300) + 0xf0;
            if compass == 0 {
                block(v, sx - 2, sy - 2, sx + 3, sy + 3, 0xff);
            } else {
                // Triangle outline (0x28408): tip 7 ahead, the other corners 7 behind at +-0.7 rad.
                let chop = |f: f64| f.trunc() as i32;
                let tip = (chop(angle.cos() * 7.0 + sx as f64), chop(angle.sin() * 7.0 + sy as f64));
                let b1 = (chop(sx as f64 - (angle - 0.7).cos() * 7.0), chop(sy as f64 - (angle - 0.7).sin() * 7.0));
                let b2 = (chop(sx as f64 - (angle + 0.7).cos() * 7.0), chop(sy as f64 - (angle + 0.7).sin() * 7.0));
                for (a, b) in [(b1, b2), (tip, b1), (tip, b2)] {
                    line(a, b, |x, y| put(v, x, y, 0xff));
                }
            }
            let black = BLACK;
            let mut tail = vec![
                Op::Fade(gfx::Fade::new(0, 10, &black, &pal)),
                Op::WaitInput { released: false },
                Op::Fade(gfx::Fade::new(0, 10, &pal, &black)),
                Op::Do(Box::new(|core: &mut Core| core.video.fill_screen(0xfe))),
            ];
            while let Some(op) = tail.pop() {
                core.ops.push_front(op);
            }
        });
    }

    fn shop_open(&mut self, core: &mut Core) {
        let from = self.island_pal();
        // 0x28ab0: fade out, picture, fade in.
        core.fade(10, &from, &BLACK);
        core.then(|core| {
            core.video.fill_screen(0);
            core.video.set_palette(&BLACK);
            core.video.clip_y0 = 0;
            core.video.clip_y1 = 0x1df;
            let pic = core.load_picture("MARCHAND.RON");
            let pal = pic.as_ref().map_or(BLACK, |p| p.pal);
            if let Some(p) = &pic {
                core.video.blit_picture(&p.pixels, p.w, p.h);
            }
            core.video.present_size(0x27f, 0x1df);
            core.stash_pal = pal;
            core.ops.push_front(Op::Fade(gfx::Fade::new(0, 10, &BLACK, &pal)));
        });
        self.hand = core.load_sheet("HAND.SPT");
        self.modal = Modal::Shop(Box::new(ShopUi {
            shop: Shop::open(&self.sim),
            x: 0x140,
            y: 0xf0,
            sel: -1,
            pal: BLACK,
            clean: Vec::new(),
            leaving: false,
            mouse: (core.input.mouse_x, core.input.mouse_y),
            drawn: (0x140, 0xf0),
        }));
    }

    /// Flashing two-line message of the shop (`0x2a320`): four times on and
    /// off, 4000 horizontal retraces each (about 9 ticks at 31.5 kHz).
    fn shop_message(core: &mut Core, l1: &'static [u8], l2: &'static [u8]) {
        for _ in 0..4 {
            core.then(move |core| {
                let font = core.font;
                let sheet = std::mem::replace(&mut core.coolfnt1, Sheet::empty());
                core.video.text(&sheet, &font, 0x1fe, 0xcc, l1, Align::Centre);
                core.video.text(&sheet, &font, 0x1fe, 0xdc, l2, Align::Centre);
                core.coolfnt1 = sheet;
                core.video.present_rect(400, 0xb4, 400 + 0xdc - 1, 0xb4 + 0x32 - 1);
            });
            core.delay(9);
            core.then(|core| {
                if let Some(clean) = core.stash_back.as_ref() {
                    for y in 0xb4..0xb4 + 0x28usize {
                        let o = y * gfx::W + 400;
                        core.video.back[o..o + 0xdc].copy_from_slice(&clean[o..o + 0xdc]);
                    }
                }
                core.video.present_rect(400, 0xb4, 400 + 0xdc - 1, 0xb4 + 0x28 - 1);
            });
            core.delay(9);
        }
    }

    /// One pass of the shop loop (one per retrace). `true` when the shop closed.
    fn shop_tick(&mut self, core: &mut Core, ui: &mut ShopUi) -> bool {
        if ui.clean.is_empty() {
            ui.clean = core.video.back.clone();
            ui.pal = core.stash_pal;
            core.stash_back = Some(ui.clean.clone());
        }
        if ui.leaving {
            // do {} while (input_any()); fade out; back to the island.
            if core.input_any() || (self.enh.menu_mouse && (core.input.mouse_left || core.input.mouse_right)) {
                return false;
            }
            let pal = ui.pal;
            core.fade(10, &pal, &BLACK);
            core.then(|core| {
                core.video.fill_screen(0xfe);
                core.stash_back = None;
            });
            self.sim.shop_exit();
            return true;
        }
        let mouse = self.enh.menu_mouse;
        if core.kbd.is_down(sc::ESC) || core.input.cancel || (mouse && core.input.mouse_right) {
            ui.leaving = true;
            return false;
        }
        let i = core.input;
        let mut click = i.secondary;
        if mouse {
            // The hand goes where the pointer goes; the keys still move it.
            let pos = (i.mouse_x, i.mouse_y);
            if pos != ui.mouse {
                ui.mouse = pos;
                ui.x = pos.0.clamp(8, 599);
                ui.y = pos.1.clamp(8, 0x1b9);
            }
            click |= i.mouse_left;
        }
        let mut zone = -1;
        if click {
            zone = shop_zones()
                .iter()
                .position(|z| ui.x >= z[0] && ui.x <= z[2] && ui.y >= z[1] && ui.y <= z[3])
                .map_or(-1, |p| p as i32);
        }
        let lang = core.settings.stp.language;
        if zone == 2 {
            ui.leaving = true;
        }
        if zone == 1 {
            ui.shop.cancel(&mut self.sim);
            match lang {
                0 => Self::shop_message(core, b"Annuler!!!", b""),
                1..=3 => Self::shop_message(core, b"Cancel!!!", b""),
                _ => {}
            }
        }
        if i.up && ui.y > 8 {
            ui.y -= 2;
        }
        if i.down && ui.y < 0x1b9 {
            ui.y += 2;
        }
        if i.left && ui.x > 8 {
            ui.x -= 2;
        }
        if i.right && ui.x < 599 {
            ui.x += 2;
        }
        if zone > 2 && zone < 0x17 && click {
            ui.sel = zone - 3;
        }
        if zone == 0 && ui.sel != -1 {
            let sel = ui.sel as usize;
            let result = ui.shop.buy(&mut self.sim, sel);
            if result != BuyResult::NotUseful {
                // The product's own jingle plays before the price check.
                let sample = match sel {
                    0 => 0x1b,
                    1 => 5,
                    2 => 0,
                    _ => 0x17,
                };
                core.sound.play(sample, 0xff);
            }
            let msg: Option<(&'static [u8], &'static [u8])> = match (result, lang) {
                (BuyResult::NotEnoughMoney, 0) => Some((b"Pas assez", b"d'argent !")),
                (BuyResult::NotEnoughMoney, 1) => Some((b"Not enough", b"money !!!")),
                (BuyResult::NotEnoughMoney, 2) => Some((b"!No tienes", b"bastante dinero!")),
                (BuyResult::NotEnoughMoney, 3) => Some((b"Not enough", b"money ???")),
                (BuyResult::Ok, 0 | 1 | 3) => Some((b"Ok!", b"")),
                (BuyResult::Ok, 2) => Some((b"!De acuerdo!", b"")),
                (BuyResult::NotUseful, 0) => Some((b"Inutile !", b"")),
                (BuyResult::NotUseful, 1 | 3) => Some((b"not useful!", b"")),
                (BuyResult::NotUseful, 2) => Some((b"!Inutil!", b"")),
                _ => None,
            };
            if let Some((a, b)) = msg {
                Self::shop_message(core, a, b);
            }
            core.sound.play(if result == BuyResult::Ok { 0xe } else { 0x19 }, 0xff);
            ui.sel = -1;
        }
        // Draw: money, selected product, hand; present the three areas; restore.
        let money = format!("{}", self.sim.s.p.money);
        let (x, y) = (ui.x, ui.y);
        let font = core.font;
        let sheet = std::mem::replace(&mut core.coolfnt1, Sheet::empty());
        core.video.text(&sheet, &font, 0x156, 0x1c4, money.as_bytes(), Align::Centre);
        core.coolfnt1 = sheet;
        if ui.sel >= 0 {
            let frame = PRODUCT_TABLE[ui.sel as usize].0;
            core.video.sprite(&self.bonus_sheet, frame.max(0) as usize, 0x140, 0x170);
        }
        core.video.sprite(&self.hand, 0, x, y);
        let rects = [(0xda, 0x1a9, 0xff, 0x23), (0xf5, 0x13c, 0xb4, 0x61), (x - 4, y - 4, 0x28, 0x28)];
        if mouse {
            // The pointer can move the hand further than its own margin.
            let (px, py) = std::mem::replace(&mut ui.drawn, (x, y));
            core.video.present_rect(px - 4, py - 4, px + 0x23, py + 0x23);
        }
        for (x0, y0, w, h) in rects {
            core.video.present_rect(x0, y0, x0 + w - 1, y0 + h - 1);
        }
        for (x0, y0, w, h) in rects {
            for yy in y0.max(0)..(y0 + h).min(gfx::H as i32) {
                let (a, b) = (x0.max(0) as usize, (x0 + w).min(gfx::W as i32) as usize);
                let o = yy as usize * gfx::W;
                core.video.back[o + a..o + b].copy_from_slice(&ui.clean[o + a..o + b]);
            }
        }
        false
    }

    // ---- rendering ---------------------------------------------------------

    /// `animaps_tick` (`0x11b40`): copy the current frame of each animated
    /// tile from `ANIMAPS.RON` into the tile sheet.
    fn animate_tiles(&mut self) {
        const PAIRS: [(usize, usize); 6] = [(0, 0), (1, 1), (2, 2), (0, 3), (3, 1498), (4, 1501)];
        let (Some(island), Some(pic)) = (self.island.as_mut(), self.animaps.as_ref()) else { return };
        for (i, (row, tile)) in PAIRS.into_iter().enumerate() {
            let t = (self.sim.s.animap[i] + 1).min(17);
            let frame = ((17 - t) / 3) as usize;
            let Some(&off) = island.uni.tile_offsets.get(tile) else { continue };
            for y in 0..31 {
                let src = (row * 32 + y) * pic.w + frame * 32;
                let dst = off as usize + y * 256;
                if src + 31 <= pic.pixels.len() && dst + 31 <= island.uni.sheet.len() {
                    island.uni.sheet[dst..dst + 31].copy_from_slice(&pic.pixels[src..src + 31]);
                }
            }
        }
    }

    /// `island_frame` tail: `render_frame` (`0x148a0`) up to the HUD. The
    /// picture reaches the screen [`World::frame_ticks`] ticks later.
    fn render(&mut self, core: &mut Core) {
        self.animate_tiles();
        if self.island.is_none() {
            return;
        }
        let s = &self.sim.s;
        self.view.begin_frame(s.p.py);
        let mut objects: Vec<ObjectInstance> = Vec::new();
        for m in &self.sim.meshes {
            let slot = m.slot.max(0) as usize;
            if objects.len() <= slot {
                objects.resize(
                    slot + 1,
                    ObjectInstance { active: false, model: 0, x: 0.0, y: 0.0, z: 0.0, rot_a: 0.0, rot_b: 0.0 },
                );
            }
            objects[slot] = ObjectInstance {
                active: true,
                model: m.model.max(0) as usize,
                x: m.x,
                y: m.y,
                z: m.z,
                rot_a: 0.0,
                rot_b: m.yaw,
            };
        }
        let hud = HudState {
            energy: s.hud_energy,
            money: s.hud_money,
            lives: s.p.lives,
            fuel: s.p.fuel,
            weapon: s.p.weapon,
            ammo: s.p.ammo[(s.p.weapon & 15).min(9) as usize],
            power: s.p.power,
            compass: s.p.compass != 0,
            angle: s.p.angle,
            boss: (s.boss_state == 1).then(|| (s.monsters.first().map_or(0, |m| m.hp), (s.difficulty + 1) * s.island * 10)),
        };
        let snap = Snap {
            cam_x: s.p.cam_x,
            cam_z: s.p.cam_z,
            angle: s.p.angle,
            key_turn: self.key_turn,
            view: self.view,
            objects,
            sprites: self.sim.sprites.clone(),
            hud,
        };
        self.view.horizon = self.draw(core, &snap);
        if self.enh.smooth_view {
            self.snap_prev = self.snap.replace(snap);
        }
        self.wait = self.frame_ticks;
    }

    /// Scene and HUD of `snap` into the renderer's back buffer. Returns the
    /// horizon row for the next frame's sky.
    fn draw(&mut self, core: &Core, snap: &Snap) -> i32 {
        let Some(island) = self.island.as_ref() else { return snap.view.horizon };
        let mode = Self::view_mode(core);
        let sprites: Vec<WorldSprite> = snap
            .sprites
            .iter()
            .map(|d| WorldSprite {
                bank: match d.bank {
                    Bank::Duck => &self.duck,
                    Bank::Boom => &self.boom,
                    Bank::Enemy => &self.enemy,
                    Bank::Boss => &self.boss,
                    Bank::Bonus => &self.bonus,
                    Bank::Icone => &self.icone,
                },
                index: d.frame.max(0) as usize,
                flags: d.flags as u32,
                x: d.x,
                y: d.y,
                z: d.z,
                angle: 0.0,
                scale: d.scale,
                anchor: Anchor::HotSpot,
            })
            .collect();
        let scene = Scene {
            island,
            mode,
            interlace: core.settings.fast_mode != 0,
            camera: Camera { x: snap.cam_x, z: snap.cam_z, heading: snap.angle },
            view: snap.view,
            objects: &snap.objects,
            sprites: &sprites,
        };
        let horizon = self.renderer.render(&scene).horizon;
        let (_, vh) = mode.size();
        let high = mode == ViewMode::High;
        let mut canvas = Canvas { pix: self.renderer.back_mut(), clip_y0: 0, clip_y1: vh - 1 };
        draw_hud(&mut canvas, &self.numbers, &snap.hud, high);
        if let Some(map) = &self.minimap {
            draw_minimap(&mut canvas, map, &self.sim.s, snap, high);
        }
        horizon
    }

    /// Between two simulation frames (`Enhancements::smooth_view`): draw the
    /// view for the moment the frontend is about to show. `phase` is the part
    /// of the current 70 Hz tick that has passed.
    pub fn render_smooth(&mut self, core: &mut Core, phase: f32) {
        if !self.enh.smooth_view || self.reinit != 0 || core.busy() || !matches!(self.modal, Modal::None) {
            return;
        }
        let (Some(cur), Some(prev)) = (self.snap.as_ref(), self.snap_prev.as_ref()) else { return };
        let done = self.frame_ticks.saturating_sub(self.wait) as f32 + phase.clamp(0.0, 1.0);
        let t = (done / self.frame_ticks as f32).clamp(0.0, 1.0);
        let cam_x = lerp_near(prev.cam_x, cur.cam_x, t);
        let cam_z = lerp_near(prev.cam_z, cur.cam_z, t);
        // Mouse movement the simulation has not seen yet turns the view already.
        let pending = if self.wants_turn() { core.mouse_dx * Self::mouse_scale(core) } else { 0.0 };
        let angle = cur.angle - cur.key_turn * (1.0 - t) + pending;
        // The queue has no identities and its order changes as things come into range,
        // so each sprite is paired with the nearest one of its kind in the previous
        // frame: something that stands still pairs with itself and does not move.
        let earlier = |c: &SpriteDraw| {
            prev.sprites
                .iter()
                .filter(|p| p.bank == c.bank && p.scale == c.scale && (p.y < 0) == (c.y < 0))
                .map(|p| (((p.x - c.x) as i64).pow(2) + ((p.z - c.z) as i64).pow(2), p))
                .min_by_key(|&(d, _)| d)
                .filter(|&(d, _)| d <= (LERP_MAX as i64).pow(2))
                .map(|(_, p)| p)
        };
        let sprites = cur
            .sprites
            .iter()
            .map(|c| {
                let mut d = *c;
                if c.scale <= -900.0 {
                    // The horizon marker ten cells ahead of the duck (`player_draw`).
                    d.x = (angle.cos() * 3000.0) as i32 - cam_x;
                    d.z = (angle.sin() * 3000.0) as i32 - cam_z;
                } else if (c.x, c.z) == (-cur.cam_x, -cur.cam_z) {
                    // The duck stays under the camera.
                    (d.x, d.z) = (-cam_x, -cam_z);
                    if let Some(p) = prev.sprites.iter().find(|p| (p.x, p.z) == (-prev.cam_x, -prev.cam_z) && p.scale == c.scale) {
                        if (p.y < 0) == (c.y < 0) {
                            d.y = lerp(p.y, c.y, t);
                        }
                    }
                } else if let Some(p) = earlier(c) {
                    (d.x, d.y, d.z) = (lerp(p.x, c.x, t), lerp(p.y, c.y, t), lerp(p.z, c.z, t));
                }
                d
            })
            .collect();
        let mut view = cur.view;
        view.eye_h = lerp(prev.view.eye_h, cur.view.eye_h, t);
        view.look_h = lerp(prev.view.look_h, cur.view.look_h, t);
        let snap = Snap {
            cam_x,
            cam_z,
            angle,
            key_turn: 0.0,
            view,
            objects: cur.objects.clone(),
            sprites,
            hud: HudState { angle, ..cur.hud },
        };
        self.draw(core, &snap);
        let mode = Self::view_mode(core);
        self.renderer.present(&mut core.video.screen, mode, core.settings.fast_mode != 0, &self.enh);
    }

    fn present(&mut self, core: &mut Core) {
        if self.reinit == 1 {
            // view_mode_apply (0x146f0): this frame is not shown.
            self.reinit = 3;
            let pal = self.island_pal();
            core.fade(0x14, &pal, &BLACK);
            core.then(|core| {
                core.video.fill_screen(crate::render::BORDER_COLOUR);
                core.video.set_palette(&BLACK);
            });
            self.renderer.reset();
            self.view.horizon = 200;
            return;
        }
        let mode = Self::view_mode(core);
        self.renderer.present(&mut core.video.screen, mode, core.settings.fast_mode != 0, &self.enh);
        self.frames += 1;
        match self.reinit {
            2 => {
                let pal = self.island_pal();
                core.fade(0x14, &BLACK, &pal);
                self.reinit = 0;
                // g_ticks = 0 once the fade is over.
                self.zero_ticks = true;
            }
            3 => self.reinit = 2,
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Wait {
    Input,
    Delay,
}

/// The common body of the picture helpers.
fn queue_picture(core: &mut Core, name: String, steps: i32, from: Pal, lines: Option<[Vec<u8>; 4]>, wait: Wait) {
    core.fade(steps, &from, &BLACK);
    core.then(move |core| {
        // vesa_set_mode clears the screen.
        core.video.fill_screen(0);
        core.video.set_palette(&BLACK);
        core.video.clip_y0 = 0;
        core.video.clip_y1 = 0x1df;
        let pic = core.load_picture(&name);
        let pal = pic.as_ref().map_or(BLACK, |p| p.pal);
        if let Some(p) = &pic {
            core.video.blit_picture(&p.pixels, p.w, p.h);
        }
        core.video.present_size(0x27f, 0x1df);
        if let Some(lines) = &lines {
            let font = core.font;
            let sheet = std::mem::replace(&mut core.coolfnt1, Sheet::empty());
            for (k, l) in lines.iter().enumerate() {
                core.video.text(&sheet, &font, 0x140, [100, 0x82, 0xa0, 0xbe][k], l, Align::Centre);
            }
            core.coolfnt1 = sheet;
            core.video.present_size(0x27f, 0x1df);
        }
        let mut tail = vec![
            Op::Fade(gfx::Fade::new(0, steps, &BLACK, &pal)),
            match wait {
                Wait::Input => Op::WaitInput { released: false },
                // delay(100)
                Wait::Delay => Op::Delay(7),
            },
            Op::Fade(gfx::Fade::new(0, steps, &pal, &BLACK)),
            Op::Do(Box::new(|core: &mut Core| core.video.fill_screen(0xfe))),
        ];
        while let Some(op) = tail.pop() {
            core.ops.push_front(op);
        }
    });
}

fn line(a: (i32, i32), b: (i32, i32), mut put: impl FnMut(i32, i32)) {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
    let (sx, sy) = (if a.0 < b.0 { 1 } else { -1 }, if a.1 < b.1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        put(x, y);
        if (x, y) == b {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// The surroundings of the duck from the map picture, one pixel per cell
/// (two cells in the 320x200 views), north as on the map screen; unexplored
/// cells stay dark. Markers as on the map screen (`map_screen`, `0x280f0`).
fn draw_minimap(c: &mut Canvas, map: &MiniMap, s: &crate::sim::State, snap: &Snap, high: bool) {
    let (n, step, x0, y0) = if high { (96, 1, 520, 64) } else { (48, 2, 260, 32) };
    // Cell of the duck: column as on the map, row in `explored` (8 more than the map row).
    let (pcx, pcz) = (128 - snap.cam_x / 300, 128 - snap.cam_z / 300);
    for j in 0..n {
        for i in 0..n {
            let (cx, cz) = (pcx + (i - n / 2) * step, pcz + (j - n / 2) * step);
            let seen = (0..256).contains(&cx)
                && (8..248).contains(&cz)
                && s.explored.get((cx * 256 + cz) as usize).is_some_and(|&e| e != 0);
            let colour = if seen { map.cells[((cz - 8) * 256 + cx) as usize] } else { map.dark };
            c.pix[(y0 + j) as usize * gfx::W + (x0 + i) as usize] = colour;
        }
    }
    let dot = |c: &mut Canvas, wx: i32, wz: i32, colour: u8| {
        // The map screen draws these two rows below their cell (0xf4 against 0xf0).
        let (i, j) = ((128 + wx / 300 - pcx) / step + n / 2, (130 + wz / 300 - pcz) / step + n / 2);
        if (1..n - 1).contains(&i) && (1..n - 1).contains(&j) {
            c.fill_rect(x0 + i - 1, y0 + j - 1, x0 + i + 1, y0 + j + 1, colour);
        }
    };
    for k in 0..10 {
        if s.q.tele_x[k] != -1 {
            dot(c, s.q.tele_x[k], s.q.tele_z[k], map.tele);
        }
    }
    for k in 0..20 {
        if s.q.hint_x[k] != -1 {
            dot(c, s.q.hint_x[k], s.q.hint_z[k], map.hint);
        }
    }
    let (mx, my) = (x0 + n / 2, y0 + n / 2);
    if snap.hud.compass {
        // The map screen's triangle, smaller.
        let a = snap.angle as f64;
        let r = if high { 5.0 } else { 3.0 };
        let at = |a: f64, r: f64| ((mx as f64 + a.cos() * r) as i32, (my as f64 + a.sin() * r) as i32);
        let (tip, b1, b2) = (at(a, r), at(a - 0.7, -r), at(a + 0.7, -r));
        for (p, q) in [(b1, b2), (tip, b1), (tip, b2)] {
            line(p, q, |x, y| c.fill_rect(x, y, x, y, map.duck));
        }
    } else {
        c.fill_rect(mx - 1, my - 1, mx + 1, my + 1, map.duck);
    }
    c.rect_outline(x0 - 1, y0 - 1, x0 + n, y0 + n, map.frame);
}

/// What the HUD shows (`hud_draw`, `0x246b0`; the counters it also updates
/// are advanced by the simulation).
#[derive(Clone, Copy)]
pub struct HudState {
    /// Displayed energy 0..=99 (`0x415fc`) and money (`0x415f8`).
    pub energy: i32,
    pub money: i32,
    pub lives: i32,
    pub fuel: i32,
    pub weapon: i32,
    /// Ammunition of the selected weapon.
    pub ammo: i32,
    pub power: i32,
    pub compass: bool,
    pub angle: f32,
    /// Boss alive: its hit points and `(difficulty + 1) * island * 10`.
    pub boss: Option<(i32, i32)>,
}

/// `hud_draw` (`0x246b0`). `high` = the 640x400 view; the 320x200 views use
/// the half-size sprites that start at index 0x2e of `NUMBERS.SPT`.
pub fn draw_hud(c: &mut Canvas, numbers: &Sheet, h: &HudState, high: bool) {
    let spr = |c: &mut Canvas, i: i32, x: i32, y: i32| c.sprite(numbers, i.max(0) as usize, x, y);
    let fuel_colour = if h.fuel < 0x4e2 {
        0x86
    } else if h.fuel < 0x9c4 {
        0x8d
    } else {
        0xa7
    };
    // 0x42 9d9: radians to degrees.
    let deg = (h.angle as f64 * 57.29577866666166).trunc() as i32;
    let needle = ((deg + 0x44) / 0x2d) & 7;
    if high {
        let mut v = h.money;
        for i in 0..7 {
            spr(c, v % 10, i * -0xf + 600, 0x1e);
            v /= 10;
        }
        spr(c, 10, 100, 0x1e);
        spr(c, h.energy % 10, 0x55, 0x1e);
        spr(c, h.energy / 10, 0x46, 0x1e);
        spr(c, (99 - h.energy) / 10 + 0xd, 0x1e, 0x37);
        spr(c, 0xc, 0x244, 0x17a);
        spr(c, 0xb, 0x20f, 0x17e);
        c.fill_rect(0x212, 0x181, h.fuel / 0x32 + 0x212, 0x189, fuel_colour);
        if h.fuel < 4999 {
            c.fill_rect(h.fuel / 0x32 + 0x213, 0x181, 0x275, 0x189, 0xc0);
        }
        for i in 0..h.lives.min(3) {
            spr(c, i + 0x17, i * 0x14 + 0x87, 0x1e);
        }
        for i in 3..h.lives.min(6) {
            spr(c, i + 0x14, (i - 3) * 0x14 + 0x81, 0x24);
        }
        if h.weapon >= 0 {
            spr(c, h.weapon + 0x1a, 0x69, 0x186);
            if h.power != 0 {
                spr(c, h.power + 0x22, 0xa0, 0x186);
            }
            if h.weapon > 0 {
                let mut v = h.ammo;
                for i in 0..4 {
                    spr(c, v % 10, (3 - i) * 0xf + 0x14, 0x186);
                    v /= 10;
                }
            }
        }
        if h.compass {
            spr(c, 0x25, 0x140, 0x32);
            spr(c, needle + 0x26, 0x140, 0x1c);
        }
        if let Some((hp, k)) = h.boss {
            spr(c, 0xb, 0xa7, 0x1bf);
            let w = hp * 100 / (k + 0x65);
            c.fill_rect(0xaa, 0x1c2, w + 0xaa, 0x1ca, 0x86);
            if hp < k + 100 {
                c.fill_rect(w + 0xab, 0x1c2, 0x10d, 0x1ca, 0xc0);
            }
        }
    } else {
        let mut v = h.money;
        for i in 0..7 {
            spr(c, v % 10 + 0x2e, i * -7 + 300, 0xf);
            v /= 10;
        }
        spr(c, 0x38, 0x31, 0xf);
        spr(c, h.energy % 10 + 0x2e, 0x2a, 0xf);
        spr(c, h.energy / 10 + 0x2e, 0x23, 0xf);
        spr(c, (99 - h.energy) / 10 + 0x3b, 0xf, 0x1b);
        spr(c, 0x3a, 0x122, 0xb9);
        spr(c, 0x39, 0x106, 0xbb);
        c.fill_rect(0x109, 0xbe, (h.fuel / 0x32 + 0x212) / 2, 0xc1, fuel_colour);
        if h.fuel < 4999 {
            c.fill_rect((h.fuel / 0x32 + 0x213) / 2, 0xbe, 0x13a, 0xc1, 0xc0);
        }
        for i in 0..h.lives.min(3) {
            spr(c, i + 0x45, i * 10 + 0x43, 0xf);
        }
        for i in 3..h.lives.min(6) {
            spr(c, i + 0x42, (i - 3) * 10 + 0x40, 0x12);
        }
        if h.weapon >= 0 {
            spr(c, h.weapon + 0x48, 0x34, 0xc3);
            if h.power != 0 {
                spr(c, h.power + 0x50, 0x50, 0xc3);
            }
            if h.weapon > 0 {
                let mut v = h.ammo;
                for i in 0..4 {
                    spr(c, v % 10 + 0x2e, ((3 - i) * 0xf + 0x14) / 2, 0xc3);
                    v /= 10;
                }
            }
        }
        if h.compass {
            spr(c, 0x53, 0xa0, 0x19);
            spr(c, needle + 0x54, 0xa0, 0xe);
        }
        if let Some((hp, k)) = h.boss {
            spr(c, 0x39, 0x53, 0xdf);
            let w = hp * 100 / (k + 0x65);
            c.fill_rect(0x55, 0xe1, (w + 0xaa) / 2, 0xe5, 0x86);
            if hp < k + 100 {
                c.fill_rect((w + 0xab) / 2, 0xe1, 0x86, 0xe5, 0xc0);
            }
        }
    }
}
