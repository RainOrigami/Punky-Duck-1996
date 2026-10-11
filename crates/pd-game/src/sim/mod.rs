//! Gameplay simulation of Punky Duck, ported from `P.EXE` (specs: `docs/re/game/`).
//!
//! Deterministic, plain data, no rendering, no audio device, no files. Everything the
//! original keeps in globals lives in [`State`]; the original save-game layout reads
//! into and writes from it ([`State::read_save`], [`State::write_save`]).
//!
//! # Surface for the app / glue code
//!
//! ## Program flow (mirrors `main`, `0x12050`)
//!
//! ```text
//! let mut sim = Sim::new(Enhancements::original());   // state of a fresh program
//! sim.s.difficulty = ...;                             // GAME.STP[5]
//! sim.new_game();                                     // NEW GAME only (new_game_reset(0))
//! sim.start_island(n, &uni, &ile0n_trj, monstern_trj);// new game or next island
//! sim.enter();                                        // EVERY entry into the frame loop:
//!                                                     //   new island, RESUME, CONTINUE, LOAD
//! loop {                                              // one rendered frame
//!     sim.frame(&input, ticks);                       // see "Timing"
//!     for e in sim.drain_events() { ... }             // sounds, dialogue, pictures, ...
//!     draw sim.sprites / sim.meshes, HUD from sim.s
//!     match sim.status() { ... }                      // death / exit / next island
//! }
//! ```
//!
//! Loading a save: `sim.load_save(&bytes, &uni, monstern_trj)?` then `sim.enter()`.
//! Saving: `sim.s.write_save()`. Continue after game over: [`Sim::continue_game`] then
//! `enter()`.
//!
//! ## Timing (the original's split, `docs/re/game/player.md` 3.1)
//!
//! The original runs a 70 Hz step loop inside each rendered frame:
//! `ticks = elapsed 70 Hz ticks + game_speed`, at least 1, and then **`ticks + 1` steps**.
//! The caller supplies `ticks` to [`Sim::frame`]; the simulation never chooses it.
//!
//! - per step ([`Sim::step`]): player movement, zone damage, drowning, weapon cooldown,
//!   firing, shot flight. Turning is `0.07 / ticks` rad per step.
//! - per frame ([`Sim::frame_begin`] before the steps, [`Sim::frame_end`] after): hurt
//!   timer, jetpack fuel, death sequence, duck sprite choice, shot explosions, all
//!   monster logic, weapon / jetpack keys, quest script, pickups, quest items, HUD
//!   counters.
//!
//! `frame(input, ticks)` is exactly `frame_begin(); ticks.max(1) + 1 x step(); frame_end()`
//! with the gameplay inputs blanked while the duck is dead, as `read_input(dead == 0)` does.
//!
//! ## Input
//!
//! [`SimInput`] is the original 16-byte input record (also the demo-file record). Level
//! keys are plain state; edge keys (jetpack, next weapon, F1, F2, map, inventory, Esc,
//! `1`, `2`) must be true only on the frame the key goes down — [`InputEdges`] does that
//! from raw key states.
//!
//! ## Outputs
//!
//! - [`Sim::events`] / [`Sim::drain_events`]: [`Event`]s in the order the original
//!   performs them. Modal events (`Say`, `Picture`, `PictureTimed`, `TextOnPicture`,
//!   `Inventory`, `Map`, `Shop`) freeze the original game while shown; the app shows
//!   them in order after the frame and before calling `frame` again. `Shop` must end
//!   with [`Sim::shop_exit`].
//! - [`Sim::sprites`]: world sprites of this frame in queue order (the arguments of the
//!   original `world_sprite_add`, `0x138f0`). The original queue holds at most 99 and
//!   drops sprites further than 4500 units from the duck on either axis; that culling
//!   is left to the renderer. `y` conventions as in the original calls: duck, player
//!   shots and monster shots pass absolute heights; monsters pass `-300` (`-y`),
//!   pickups `-600` / `-3000`, quest items `-300` (negative = relative to the terrain).
//! - [`Sim::meshes`]: `OBJETS_n.3D` instances of this frame (arguments of
//!   `object3d_place`, `0x14050`).
//! - [`Sim::s`]: all state, for HUD (`hud_energy`, `hud_money`, `p.lives`, `p.fuel`,
//!   `p.weapon`, `p.power`, `p.ammo`, `p.compass`, boss `monsters[0].hp`), map screen
//!   (`explored`, `q.tele_*`, `q.hint_*`, `p.rx/rz`, `p.angle`), inventory (`q.inv`,
//!   `q.ninv`), animated tiles (`animap`).
//! - [`Sim::status`]: what `main` checks after each frame.
//!
//! ## Shop
//!
//! `Event::Shop` is raised when the duck's target enters zone 18. The app runs the
//! modal screen and calls [`shop::Shop::open`], [`shop::Shop::buy`],
//! [`shop::Shop::cancel`] and finally [`Sim::shop_exit`].
//!
//! # Fidelity
//!
//! Original bugs are reproduced; each fix is a named field of
//! [`crate::settings::Enhancements`]. Floating point follows the x87 code with `f64`
//! intermediates and truncating conversions (`__CHP`, `0x305a4`).

pub mod rng;
pub mod save;
pub mod shop;
pub mod state;
pub mod tables;
pub mod terrain;

mod monsters;
mod pickups;
mod player;
mod scripts;

use crate::settings::Enhancements;
use pd_formats::trj::Trj;
use pd_formats::uni::Uni;

pub use save::{SaveError, SAVE_SIZE};
pub use state::{Bonus, IslandFlags, Item, Monster, Player, Quest, Shots, State};
pub use terrain::Terrain;

/// The original 16-byte input record at `0x50d5c4` (`docs/re/engine/input.md`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimInput {
    /// Level keys.
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// Strafe modifier (Ctrl).
    pub strafe: bool,
    /// Edge: Enter, jetpack toggle.
    pub jetpack: bool,
    /// Edge: Backspace, next weapon.
    pub next_weapon: bool,
    /// Edge: F1 view size, F2 interlace.
    pub view: bool,
    pub interlace: bool,
    /// Level: Space, fire.
    pub fire: bool,
    /// Edge: Tab, map.
    pub map: bool,
    /// Edge: inventory keys.
    pub inventory: bool,
    /// Edge: Esc.
    pub esc: bool,
    /// Edge: `1` / `2`, quest pictures.
    pub key1: bool,
    pub key2: bool,
}

impl SimInput {
    /// Decode a demo-file record.
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        SimInput {
            up: b[0] != 0,
            down: b[1] != 0,
            left: b[2] != 0,
            right: b[3] != 0,
            strafe: b[4] != 0,
            jetpack: b[5] != 0,
            next_weapon: b[6] != 0,
            view: b[7] != 0,
            interlace: b[8] != 0,
            fire: b[9] != 0,
            map: b[10] != 0,
            inventory: b[11] != 0,
            esc: b[12] != 0,
            key1: b[13] != 0,
            key2: b[14] != 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let f = [
            self.up, self.down, self.left, self.right, self.strafe, self.jetpack,
            self.next_weapon, self.view, self.interlace, self.fire, self.map, self.inventory,
            self.esc, self.key1, self.key2, false,
        ];
        f.map(|v| v as u8)
    }

    /// What `read_input(0)` leaves: only F1, F2 and Esc (bytes 7, 8, 12).
    pub fn without_gameplay(&self) -> Self {
        SimInput { view: self.view, interlace: self.interlace, esc: self.esc, ..Default::default() }
    }
}

/// Edge detection for the edge-triggered bytes, as `read_input` does with its
/// previous-state bytes: feed raw key states once per frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputEdges {
    prev: SimInput,
}

impl InputEdges {
    /// `held`: every field is the raw "key is down" state. Returns the record with the
    /// edge fields true only when newly pressed.
    pub fn poll(&mut self, held: &SimInput) -> SimInput {
        let p = self.prev;
        self.prev = *held;
        SimInput {
            jetpack: held.jetpack && !p.jetpack,
            next_weapon: held.next_weapon && !p.next_weapon,
            view: held.view && !p.view,
            interlace: held.interlace && !p.interlace,
            map: held.map && !p.map,
            inventory: held.inventory && !p.inventory,
            esc: held.esc && !p.esc,
            key1: held.key1 && !p.key1,
            key2: held.key2 && !p.key2,
            ..*held
        }
    }
}

/// Sprite bank of a world sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bank {
    /// `DUCK.SPT` (`DUCK_1.SPT` on island 5 after `Event::LoadDuckBank`).
    Duck,
    /// `BOOM.SPT`.
    Boom,
    /// `ENEMYn.SPT`.
    Enemy,
    /// `BOSSn.SPT`.
    Boss,
    /// `BONUS_n.SPT`.
    Bonus,
    /// `ICONE_n.SPT`.
    Icone,
}

/// One `world_sprite_add(frame, bank, flags, x, y, z, 0, scale, 0)` call (`0x138f0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDraw {
    pub frame: i32,
    pub bank: Bank,
    /// Bit 0: mirrored; bit 3 (8): monster was hit this frame.
    pub flags: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Scale argument as passed: 4.0 normal, 6.0 explosions, 12.0 boss, 8.0 boss shots,
    /// -1.0 the duck, -999.0 the marker ahead of the duck.
    pub scale: f32,
}

/// One `object3d_place(slot, model, x, y, z, rx, ry, rz, 1)` call (`0x14050`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshDraw {
    pub slot: i32,
    pub model: i32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Yaw in radians (seventh argument).
    pub yaw: f32,
}

/// Things the simulation asks the rest of the program to do, in original order.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `sfx(sample, volume)` (`0x2ef10`); sample = index into the name table at
    /// `0x41a84`, volume 0..255 (may be out of range as computed; clamp).
    Sfx { sample: i32, volume: i32 },
    /// Volume of the sea loop on channel 0 (`0x2ef70`), once per frame.
    SeaVolume(i32),
    /// `cd_stop(); cd_play(track)` if music is on.
    CdTrack(i32),
    /// Request a fade-in of the game palette (`[0x40058] = 1`).
    FadeIn,
    /// Modal: dialogue line of `ILE_n.TXT` (`say`, `0x28e00`).
    Say(i32),
    /// Modal: full-screen picture until a key (`0x28750`).
    Picture(&'static str),
    /// Modal: picture with fades of `steps` palette steps and a fixed `delay(100)`,
    /// no key (`0x28990`).
    PictureTimed(&'static str, i32),
    /// Modal: dialogue line drawn on a picture (`0x290e0`).
    TextOnPicture(i32, &'static str),
    /// Modal screens requested by key.
    Inventory,
    Map,
    /// Modal: the shop; finish with [`Sim::shop_exit`].
    Shop,
    /// Load `BOSS<island>.SPT` into the boss bank.
    LoadBossBank,
    /// Replace the duck bank (island 5 commando outfit).
    LoadDuckBank(&'static str),
    /// F1 pressed (cycle view size; sample 12 already requested).
    ViewCycle,
    /// F2 pressed (interlace toggle, only meaningful in the 640x400 view; the original
    /// plays sample 12 only when it applies).
    InterlaceToggle,
}

/// What `main` looks at after each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    /// `lives < 0`: game over screen.
    GameOver,
    /// Leave for the next island (3..=5, or 2 with the island-1 cut disabled).
    NextIsland(i32),
    /// Play the ending.
    Ending,
}

/// Static inputs of one island.
/// Input that has no place in the original 16-byte record. Only read when
/// [`Enhancements::modern_controls`] is on; set it before [`Sim::frame`].
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ModernInput {
    /// Heading change for this frame in radians (positive turns right), spread
    /// evenly over the frame's steps.
    pub turn: f32,
    /// Strafe without the modifier key.
    pub strafe_left: bool,
    pub strafe_right: bool,
    /// Edge: select this weapon (0..=9) if it is owned and has ammunition.
    pub select_weapon: Option<u8>,
}

pub struct Sim {
    /// See [`ModernInput`].
    pub modern: ModernInput,
    pub s: State,
    pub terrain: Terrain,
    /// `MONSTERn.TRJ` of the current island.
    pub monster_trj: Trj,
    pub enh: Enhancements,
    pub events: Vec<Event>,
    pub sprites: Vec<SpriteDraw>,
    pub meshes: Vec<MeshDraw>,
    /// Input of the current frame as the script and key handlers see it.
    pub(crate) input: SimInput,
    /// Stand-in for the stale register the cluster explosion writes through (see
    /// `player.rs`): slot index of the previous cluster write in this step, if any.
    pub(crate) cluster_edi: Option<usize>,
}

fn empty_trj() -> Trj {
    Trj { paths: Vec::new(), records: Vec::new(), points: Vec::new() }
}

/// `trunc()` of the specs: x87 round-toward-zero then store as i32.
#[inline]
pub(crate) fn chop(v: f64) -> i32 {
    // `fistp` of an out-of-range value stores 0x80000000.
    if v.is_nan() || v >= 2147483648.0 || v <= -2147483649.0 {
        i32::MIN
    } else {
        v.trunc() as i32
    }
}

impl Sim {
    /// The state of a freshly started program.
    pub fn new(enh: Enhancements) -> Self {
        Sim {
            modern: ModernInput::default(),
            s: State::default(),
            terrain: Terrain::default(),
            monster_trj: empty_trj(),
            enh,
            events: Vec::new(),
            sprites: Vec::new(),
            meshes: Vec::new(),
            input: SimInput::default(),
            cluster_edi: None,
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.events)
    }

    pub(crate) fn sfx(&mut self, sample: i32, volume: i32) {
        self.events.push(Event::Sfx { sample, volume });
    }

    pub(crate) fn sprite(&mut self, frame: i32, bank: Bank, flags: i32, x: i32, y: i32, z: i32, scale: f32) {
        self.sprites.push(SpriteDraw { frame, bank, flags, x, y, z, scale });
    }

    pub(crate) fn mesh(&mut self, slot: i32, model: i32, x: f32, y: f32, z: f32, yaw: f32) {
        self.meshes.push(MeshDraw { slot, model, x, y, z, yaw });
    }

    /// `new_game_reset(0)` (`0x292d0`): NEW GAME. Call before [`Sim::start_island`].
    pub fn new_game(&mut self) {
        let p = &mut self.s.p;
        p.continues = 0;
        p.money = 0;
        p.power = 0;
        p.weapon = 0;
        p.fuel = 0;
        p.lives = 3;
        p.energy = 99;
        p.ammo = [0; 10];
        p.ammo[0] = 10_000_000;
        p.owned = [0; 10];
        p.owned[0] = 1;
    }

    /// The per-island part of `new_game_reset` (argument 1).
    fn island_transient_reset(&mut self) {
        let s = &mut self.s;
        s.p.shooting = 0;
        s.p.flying = 0;
        s.p.hurt = 0;
        s.p.strafe = 0;
        s.p.drown = 0;
        s.p.swim_timer = 0;
        s.q.flags.flag70 = 0;
        s.q.touch = -1;
        s.q.touch_slot = 0;
        s.p.dead = 0;
        s.p.dead_wait = 0;
        s.next_island = 0;
        s.q.model = -1;
        s.q.model2 = -1;
        s.p.compass = 0;
        s.boss_state = 0;
        s.q.flags.unk_e74 = 0;
        s.q.flags.flag90 = 0;
        s.q.flags.unk_e94 = 0;
    }

    /// Start an island from scratch: `island_reset[n]()` (monster tables, TRJ, spawn),
    /// `new_game_reset(1)`, `island_start[n]()` (start position, pickups, random
    /// money, quest items). Consumes random numbers. Follow with [`Sim::enter`].
    pub fn start_island(&mut self, island: i32, uni: &Uni, bonus_trj: &Trj, monster_trj: Trj) {
        assert!((1..=5).contains(&island));
        self.s.island = island;
        self.terrain = Terrain::from_uni(uni);
        self.monster_trj = monster_trj;
        // island_reset: class bytes, TRJ, monsters, sprite tables.
        let k = (island - 1) as usize;
        self.s.class_bytes[..4].copy_from_slice(&tables::CLASS_OF[k]);
        self.s.nmonsters = self.monster_trj.records.len() as i32;
        self.monsters_init();
        self.s.sprite_tables = tables::MONSTER_SPRITES[k];
        self.island_transient_reset();
        self.island_start(bonus_trj);
    }

    /// Gameplay effects of the island asset function (`0x174c0` and siblings), which
    /// the original runs on **every** entry into the frame loop: new island, RESUME
    /// GAME, CONTINUE and LOAD GAME.
    pub fn enter(&mut self) {
        let keep_compass = self.enh.keep_compass_on_reentry;
        let s = &mut self.s;
        s.bonus_frames = tables::bonus_frames_init();
        s.p.walk = 1_000_000_000;
        s.p.strafe_ctr = 1_000_000_000;
        s.p.slide = tables::SLIDE[(s.island - 1).clamp(0, 4) as usize];
        s.q.busy = 0;
        s.q.touch = -1;
        s.q.touch_slot = 0;
        s.p.hurt = 0;
        s.p.drown = 0;
        s.next_island = 0;
        if !keep_compass {
            // Original: the compass is lost on every (re-)entry, including RESUME GAME
            // after Esc and LOAD GAME.
            s.p.compass = 0;
        }
        s.shots.state = [0; state::SHOTS];
        s.shots.x = [0; state::SHOTS];
        s.shots.y = [0; state::SHOTS];
        s.shots.z = [0; state::SHOTS];
        s.shots.vx = [0; state::SHOTS];
        s.shots.vz = [0; state::SHOTS];
        s.shots.ty = [0; state::SHOTS];
        if s.island == 5 {
            // The asset function reloads DUCK.SPT and clears the flag; the script
            // swaps the bank again on the next frame.
            s.q.outfit_on = 0;
        }
    }

    /// Load a save written by the original (or by [`State::write_save`]) and rebuild
    /// what `0x264f0` rebuilds (`trj_load_monsters`, monster pointers).
    pub fn load_save(&mut self, data: &[u8], uni: &Uni, monster_trj: Trj) -> Result<(), SaveError> {
        self.s.read_save(data)?;
        self.terrain = Terrain::from_uni(uni);
        self.monster_trj = monster_trj;
        self.s.nmonsters = self.monster_trj.records.len() as i32;
        for i in 0..(self.s.nmonsters as usize).min(state::MONSTERS) {
            let (path, rec) = self.monster_ptrs(i);
            self.s.monsters[i].path_ptr = path;
            self.s.monsters[i].rec_ptr = rec;
        }
        Ok(())
    }

    /// CONTINUE after game over (`main`, `0x12050`): keeps quest progress.
    pub fn continue_game(&mut self) {
        self.s.p.energy = 99;
        self.s.p.lives = 3;
        self.s.p.continues -= 1;
    }

    /// One rendered frame with `ticks + 1` steps (`frame_common` + `island_frame` +
    /// the shop check). `ticks` below 1 counts as 1, as in `tick_loop`.
    pub fn frame(&mut self, input: &SimInput, ticks: i32) {
        self.frame_begin();
        let ticks = ticks.max(1);
        // read_input(dead == 0): gameplay bytes stay zero while the duck is dead.
        let inp = if self.s.p.dead == 0 { *input } else { input.without_gameplay() };
        if self.s.p.dead != 0 || !self.enh.modern_controls {
            self.modern = ModernInput::default();
        }
        self.s.p.ticks = ticks;
        for _ in 0..=ticks {
            self.step(&inp);
        }
        self.frame_end(&inp);
    }

    /// Start of `frame_common` (`0x2a260`): hurt timer and jetpack fuel.
    pub fn frame_begin(&mut self) {
        let p = &mut self.s.p;
        if p.hurt > 0 {
            if p.hurt == 19 {
                self.events.push(Event::Sfx { sample: 2, volume: 255 });
            }
            p.hurt -= 1;
        }
        if p.fuel > 0 && p.flying == 1 {
            p.fuel -= 5;
        }
        if p.fuel < 1 && p.flying == 1 {
            p.flying = 0;
            p.fuel = 0;
        }
    }

    /// One 70 Hz step: clears the sprite and mesh queues, `player_move` (`0x10860`),
    /// `player_shots` (`0x103b0`). `self.s.p.ticks` must hold the frame's tick count.
    pub fn step(&mut self, input: &SimInput) {
        self.sprites.clear();
        self.meshes.clear();
        self.player_move(input);
        self.player_shots(input);
    }

    /// Rest of the frame after the steps: `player_draw`, shot explosions, monsters,
    /// view keys, then `island_frame` (sea volume, quest script, inventory key,
    /// animated tiles, pickups, quest items, map key, fixed meshes, HUD counters) and
    /// the shop check.
    pub fn frame_end(&mut self, input: &SimInput) {
        self.input = *input;
        self.player_draw();
        self.shot_explosions();
        self.monsters_frame();
        self.view_keys();

        self.sea_volume();
        self.run_script();
        if self.input.inventory {
            self.events.push(Event::Inventory);
        }
        self.animaps_tick();
        self.pickups_update();
        self.items_update();
        if self.input.map {
            self.events.push(Event::Map);
        }
        self.fixed_meshes();
        self.render_side_effects();
        if self.s.p.zone == 18 {
            self.events.push(Event::Shop);
        }
    }

    /// What `main` decides after a frame. Esc is the caller's business.
    pub fn status(&self) -> Status {
        if self.s.p.lives < 0 {
            return Status::GameOver;
        }
        match self.s.next_island {
            0 => Status::Playing,
            // 0x124ea: `if (next_island == 2) next_island = 6;` - island 1 ends the game.
            2 if !self.enh.island1_leads_to_island2 => Status::Ending,
            6 => Status::Ending,
            n => Status::NextIsland(n),
        }
    }

    /// `0x188e0`: sea loop volume from the zone-1 cells in the 30x30 window of the
    /// previous frame.
    fn sea_volume(&mut self) {
        let mut n = 0;
        for x in self.s.win_x..=self.s.win_x + 29 {
            for z in self.s.win_z..=self.s.win_z + 29 {
                if self.terrain.zone_flat(x * 256 + z) == 1 {
                    n += 1;
                }
            }
        }
        self.events.push(Event::SeaVolume(n.min(400) * 255 / 400));
    }

    /// `0x11b40`: animated terrain tiles. Consumes one random number per frame.
    fn animaps_tick(&mut self) {
        let r = self.s.rng.next();
        for (i, t) in self.s.animap.iter_mut().enumerate() {
            if r % 20 == i as i32 && *t == 0 {
                *t = 17;
            }
            if *t > 0 {
                *t -= 1;
            }
        }
    }

    fn fixed_meshes(&mut self) {
        let island = self.s.island;
        for &(slot, model, x, z, yaw) in tables::fixed_meshes(island) {
            if island == 2 && slot == 6 && self.s.q.flags.flag70 != 0 {
                continue;
            }
            self.mesh(slot, model, x, 0.0, z, yaw);
        }
    }

    /// Gameplay-visible work the original does while rendering: the terrain window
    /// (`0x128c0`: origin from the camera cell, marks `explored`) and the HUD counters
    /// (`0x246b0`).
    fn render_side_effects(&mut self) {
        let s = &mut self.s;
        s.win_x = 128 - s.p.cam_x / terrain::CELL - 15;
        s.win_z = 128 - s.p.cam_z / terrain::CELL - 15;
        for x in s.win_x..s.win_x + 30 {
            for z in s.win_z..s.win_z + 30 {
                let i = x * 256 + z;
                if (0..0x10000).contains(&i) {
                    s.explored[i as usize] = 1;
                }
            }
        }
        self.hud_update();
    }

    /// The state changes inside `hud` (`0x246b0`): displayed energy and money chase
    /// the real values three times per frame, plus the clamps and the weapon
    /// auto-advance that live there.
    fn hud_update(&mut self) {
        let fix_double = self.enh.fix_hud_double_life_loss;
        let s = &mut self.s;
        for _ in 0..3 {
            if s.hud_energy < s.p.energy {
                s.hud_energy += 1;
            }
            if s.p.energy < s.hud_energy {
                s.hud_energy -= 1;
            }
            let m = s.p.money;
            for (lim, stepv) in [(9999, 1000), (999, 100), (99, 10), (9, 1), (0, 1)] {
                if m.wrapping_sub(s.hud_money) > lim {
                    s.hud_money += stepv;
                }
            }
            for (lim, stepv) in [(-9999, 1000), (-999, 100), (-99, 10), (-9, 1), (0, 1)] {
                if m.wrapping_sub(s.hud_money) < lim {
                    s.hud_money -= stepv;
                }
            }
        }
        if s.p.money == 0 {
            s.hud_money = 0;
        }
        if s.hud_energy < 0 {
            // Original quirk: the HUD itself takes a life when the displayed value
            // goes negative (on top of the death sequence's own deduction).
            if !fix_double {
                s.p.lives -= 1;
            }
            s.hud_energy = 99;
        }
        if s.hud_energy > 99 {
            s.hud_energy = 99;
        }
        if s.p.lives > 5 {
            s.p.lives = 5;
        }
        if s.p.fuel > 4999 {
            s.p.fuel = 4999;
        }
        if s.p.ammo[(s.p.weapon & 15).min(9) as usize] == 0 {
            for _ in 0..8 {
                let w = (s.p.weapon & 15).min(9) as usize;
                if s.p.ammo[w] == 0 || s.p.owned[w] == 0 {
                    s.p.weapon += 1;
                }
                s.p.weapon &= 7;
            }
        }
    }

    /// In-game cheats (`read_input`, `0x26ec0`): latched flags re-applied every frame.
    /// Bits as in `0x4191a`: 0x1 lives, 0x2 energy, 0x4 ammo, 0x8 fuel, 0x10 power,
    /// 0x80 compass, 0x100 money.
    pub fn apply_cheats(&mut self, flags: u16) {
        let p = &mut self.s.p;
        if flags & 0x1 != 0 {
            p.lives = 3;
        }
        if flags & 0x2 != 0 {
            p.energy = 99;
        }
        if flags & 0x4 != 0 {
            for w in 0..8 {
                p.ammo[w] = 9999;
                p.owned[w] = 1;
            }
        }
        if flags & 0x8 != 0 {
            p.fuel = 5000;
        }
        if flags & 0x10 != 0 {
            p.power = 2;
        }
        if flags & 0x80 != 0 {
            p.compass = 1;
        }
        if flags & 0x100 != 0 {
            p.money = 500_000;
        }
    }
}
