//! Plain-data simulation state, mirroring the original's globals (`docs/re/game/globals.md`).
//!
//! Field meanings and units are the original's: the player position is stored
//! **negated** (`px = -worldX`), monsters / shots / pickups / items use true world
//! coordinates, timers count steps or frames as documented per field. Every field that
//! the original writes to a save game is here, so `sim::save` can read and write the
//! original `SAVEGAME.__n` layout.

use super::rng::Rng;
use super::tables;

pub const MONSTERS: usize = 210;
pub const SHOTS: usize = 250;
pub const BONUSES: usize = 1600;
pub const ITEMS: usize = 30;
pub const INVENTORY: usize = 20;

/// Run-time address of the `MONSTERn.TRJ` buffer (`0x4f6144` in `P.elf`) as seen in a
/// save written by the original under DOS/4GW in DOSBox-X (`out/ref`). Only used to
/// give the two stale pointers in each monster record the value the original would
/// write; the original re-derives them after loading.
pub const TRJ_BUFFER_ADDR: u32 = 0x006b_a144;

/// The duck. All i32 unless noted; `S` = part of a save game.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// S. Position, negated world X / Z (`0x517edc`, `0x517ee4`).
    pub px: i32,
    pub pz: i32,
    /// S. Height of the duck sprite (`0x517ed8`).
    pub py: i32,
    /// S. Movement target the position chases (`0x517f00`, `0x517f14`).
    pub tx: i32,
    pub tz: i32,
    /// Copy of `px`, `pz` made at the end of each step (`0x517f9c`, `0x517f98`).
    pub rx: i32,
    pub rz: i32,
    /// Camera anchor: `rx`, 6000, `rz` (`0x517f8c/90/94`).
    pub cam_x: i32,
    pub cam_y: i32,
    pub cam_z: i32,
    /// S. Facing in radians (`0x517fa0`). Forward in world space is `(cos, sin)`.
    pub angle: f32,
    /// Smoothing divisor, per island 8/8/15/40/8 (`0x517f84`).
    pub slide: i32,
    /// Zone under the duck / under the target after the last step (`0x517f80`).
    pub zone: i32,
    /// S. Jetpack active 0/1.
    pub flying: i32,
    /// S. Jetpack fuel 0..4999.
    pub fuel: i32,
    /// S. Hurt timer: 20 on a hit, -1 per frame.
    pub hurt: i32,
    /// S.
    pub energy: i32,
    pub lives: i32,
    pub continues: i32,
    pub money: i32,
    /// S. Death sequence counters (`0x517ec0`, `0x517ed0`).
    pub dead: i32,
    pub dead_wait: i32,
    /// S. Respawn position (negated coordinates).
    pub start_x: i32,
    pub start_z: i32,
    /// S. Steps of deep-water immunity left.
    pub swim_timer: i32,
    /// Deep-water step counter (`0x517f60`).
    pub drown: i32,
    /// Walk / strafe animation counters (`0x517f68`, `0x517f54`), 1e9 at island entry.
    pub walk: i32,
    pub strafe_ctr: i32,
    /// +1 after moving forward, -1 after moving backward (`0x517f64`).
    pub last_dir: i32,
    /// This step: +1 forward, -1 backward, 0 none (`0x517f7c`).
    pub move_dir: i32,
    /// This step: 0, 1 left, 2 right (`0x517f74`).
    pub strafe: i32,
    /// This step: position changed (`0x517f5c`).
    pub moved: i32,
    /// S. Decor collision of this frame: 0, 2 flying, 3 on foot (`0x517eb0`).
    pub bump: i32,
    /// S. 2 when a shot is fired, -1 per frame (`0x517f1c`).
    pub shooting: i32,
    /// S. Selected weapon 0..7, power level 0..2, shots left at this power.
    pub weapon: i32,
    pub power: i32,
    pub power_shots: i32,
    /// S.
    pub ammo: [i32; 10],
    pub owned: [i32; 10],
    /// Steps since the last shot / weapon change (`0x419ac`).
    pub fire_ctr: i32,
    /// `0x40028`, -1 at program start.
    pub last_weapon: i32,
    /// S. Has compass.
    pub compass: i32,
    /// Tick count of this frame as the original stores it (`0x517f70`): steps - 1.
    pub ticks: i32,
    /// Wading footstep latch (`0x1c6edc`).
    pub wade_latch: i32,
    /// S, never used by the code reviewed (`0x517efc`, `0x517ebc`, `0x517f78`).
    pub unk_517efc: i32,
    pub unk_517ebc: i32,
    pub unk_517f78: i32,
}

/// Player shot pool, structure of arrays as in the original.
#[derive(Clone, Debug, PartialEq)]
pub struct Shots {
    /// 0 free; 1..99 flying (steps left); 200 -> 196 exploding; 201..209 delayed
    /// explosion; -1 spent (original quirk, still hits monsters).
    pub state: [i32; SHOTS],
    pub x: [i32; SHOTS],
    pub y: [i32; SHOTS],
    pub z: [i32; SHOTS],
    pub vx: [i32; SHOTS],
    pub vz: [i32; SHOTS],
    /// Weapon index. The first 50 entries are in a save game.
    pub ty: [i32; SHOTS],
    /// Ring allocation cursor (`0x517ea8`).
    pub next: i32,
}

/// One monster slot, 0x184 bytes in the original (`0x4f8944`). Slot 0 doubles as the boss.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Monster {
    /// 0 free; 1..4 monster type; 5 boss.
    pub ty: i32,
    pub hp: i32,
    /// 0 none (dying), 1 none (boss), 5 follow TRJ path.
    pub move_kind: i32,
    /// 0 idle, 1 chase, 2 attack, 3 return, 4 dying.
    pub ai: i32,
    pub x: i32,
    pub z: i32,
    pub y: i32,
    pub home_x: i32,
    pub home_z: i32,
    pub unk_24: i32,
    /// Squared radii / 1000.
    pub detect: i32,
    pub attack: i32,
    pub speed_x: i32,
    pub speed_z: i32,
    pub dist2: i32,
    pub leash: i32,
    pub prev_x: i32,
    pub prev_z: i32,
    pub prev_y: i32,
    pub hit: i32,
    pub frame: i32,
    pub mirror: i32,
    /// 0 walk, 1 attack, 2 stand, 3 death.
    pub anim: i32,
    pub dir: i32,
    pub anim_idx: i32,
    pub anim_tick: i32,
    pub anim_period: i32,
    pub unk_6c: i32,
    pub unk_70: i32,
    pub unk_74: i32,
    pub max_shots: i32,
    pub fire_chance: i32,
    /// 0 free; > 0 flying (age in frames); < 0 impact animation (starts at -100).
    pub shot_state: [i32; 5],
    pub shot_x: [i32; 5],
    pub shot_z: [i32; 5],
    pub shot_y: [i32; 5],
    pub shot_vx: [i32; 5],
    pub shot_vz: [i32; 5],
    pub shot_vy: [i32; 5],
    pub shot_speed: i32,
    pub unk_110: i32,
    pub shot_anim: [i32; 5],
    pub spread: i32,
    pub shot_frame: [i32; 5],
    pub shot_mirror: [i32; 5],
    /// Stale pointers into the TRJ buffer as stored in save games.
    pub path_ptr: u32,
    pub rec_ptr: u32,
    pub path_idx: i32,
    pub seg_x0: i32,
    pub seg_z0: i32,
    pub seg_x1: i32,
    pub seg_z1: i32,
    pub step_x: i32,
    pub step_z: i32,
    pub seg_len: i32,
    pub rot_cos: f32,
    pub rot_sin: f32,
}

/// Size of one monster record in a save game.
pub const MONSTER_BYTES: usize = 0x184;

impl Monster {
    pub fn to_words(&self) -> [u32; 97] {
        let mut w = [0u32; 97];
        let mut i = 0;
        let mut put = |v: u32| {
            w[i] = v;
            i += 1;
        };
        for v in [
            self.ty, self.hp, self.move_kind, self.ai, self.x, self.z, self.y, self.home_x,
            self.home_z, self.unk_24, self.detect, self.attack, self.speed_x, self.speed_z,
            self.dist2, self.leash, self.prev_x, self.prev_z, self.prev_y, self.hit, self.frame,
            self.mirror, self.anim, self.dir, self.anim_idx, self.anim_tick, self.anim_period,
            self.unk_6c, self.unk_70, self.unk_74, self.max_shots, self.fire_chance,
        ] {
            put(v as u32);
        }
        for a in [
            &self.shot_state, &self.shot_x, &self.shot_z, &self.shot_y, &self.shot_vx,
            &self.shot_vz, &self.shot_vy,
        ] {
            for &v in a {
                put(v as u32);
            }
        }
        put(self.shot_speed as u32);
        put(self.unk_110 as u32);
        for &v in &self.shot_anim {
            put(v as u32);
        }
        put(self.spread as u32);
        for &v in self.shot_frame.iter().chain(&self.shot_mirror) {
            put(v as u32);
        }
        put(self.path_ptr);
        put(self.rec_ptr);
        for v in [
            self.path_idx, self.seg_x0, self.seg_z0, self.seg_x1, self.seg_z1, self.step_x,
            self.step_z, self.seg_len,
        ] {
            put(v as u32);
        }
        put(self.rot_cos.to_bits());
        put(self.rot_sin.to_bits());
        debug_assert_eq!(i, 97);
        w
    }

    pub fn from_words(w: &[u32; 97]) -> Self {
        let g = |i: usize| w[i] as i32;
        let a5 = |i: usize| [g(i), g(i + 1), g(i + 2), g(i + 3), g(i + 4)];
        Monster {
            ty: g(0),
            hp: g(1),
            move_kind: g(2),
            ai: g(3),
            x: g(4),
            z: g(5),
            y: g(6),
            home_x: g(7),
            home_z: g(8),
            unk_24: g(9),
            detect: g(10),
            attack: g(11),
            speed_x: g(12),
            speed_z: g(13),
            dist2: g(14),
            leash: g(15),
            prev_x: g(16),
            prev_z: g(17),
            prev_y: g(18),
            hit: g(19),
            frame: g(20),
            mirror: g(21),
            anim: g(22),
            dir: g(23),
            anim_idx: g(24),
            anim_tick: g(25),
            anim_period: g(26),
            unk_6c: g(27),
            unk_70: g(28),
            unk_74: g(29),
            max_shots: g(30),
            fire_chance: g(31),
            shot_state: a5(32),
            shot_x: a5(37),
            shot_z: a5(42),
            shot_y: a5(47),
            shot_vx: a5(52),
            shot_vz: a5(57),
            shot_vy: a5(62),
            shot_speed: g(67),
            unk_110: g(68),
            shot_anim: a5(69),
            spread: g(74),
            shot_frame: a5(75),
            shot_mirror: a5(80),
            path_ptr: w[85],
            rec_ptr: w[86],
            path_idx: g(87),
            seg_x0: g(88),
            seg_z0: g(89),
            seg_x1: g(90),
            seg_z1: g(91),
            step_x: g(92),
            step_z: g(93),
            seg_len: g(94),
            rot_cos: f32::from_bits(w[95]),
            rot_sin: f32::from_bits(w[96]),
        }
    }
}

/// Entry of the pickup / decor list (`0x50ea50`, 20 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Bonus {
    /// Kind 0..54, -1 = gone.
    pub ty: i32,
    pub anim: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Quest item slot (`0x516ae8`, 24 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Item {
    /// Item id = `ICONE_n.SPT` frame; -1 = free slot.
    pub id: i32,
    /// Only compared with 100 (>= 100: cannot be touched).
    pub icon: i32,
    /// Sixth `item_add` argument, never read.
    pub p6: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Per-island booleans saved in a save game, in save order
/// (`0x517e9c, e90, e74, e70, e94, e98, ea0, e8c, e80, e88, e84, e7c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IslandFlags {
    /// Island 1: build site seen (`0x517e9c`).
    pub site_seen: i32,
    /// Island 2: build site marker placed (`0x517e90`).
    pub flag90: i32,
    pub unk_e74: i32,
    /// Island 2: stolen item taken (`0x517e70`).
    pub flag70: i32,
    pub unk_e94: i32,
    pub unk_e98: i32,
    pub unk_ea0: i32,
    pub unk_e8c: i32,
    /// Island 5: vessel destroyed flags for zones 21, 22, 23, 24.
    pub vessel: [i32; 4],
}

/// Quest script state.
#[derive(Clone, Debug, PartialEq)]
pub struct Quest {
    /// S. Progress counter, -1 at island start (`0x517f2c`).
    pub step: i32,
    /// S. Last step the jingle was played for (`0x517f50`).
    pub step_shown: i32,
    /// S. 1 after a refusal until the duck leaves the item (`0x517f40`).
    pub busy: i32,
    /// S. Id of the touched quest item, -1 none; from the previous frame (`0x517f28`).
    pub touch: i32,
    /// S. Its slot (`0x517eac`).
    pub touch_slot: i32,
    /// S. Inventory, item ids in pickup order.
    pub inv: [i32; INVENTORY],
    pub ninv: i32,
    /// S. Mesh shown at the build site(s), -1 none (`0x517f30`, `0x517f4c`).
    pub model: i32,
    pub model2: i32,
    /// S. "Must finish the mission" shown (`0x517ef4`).
    pub boss_warned: i32,
    /// S.
    pub flags: IslandFlags,
    /// Island 5 commando outfit bank loaded (`0x517e78`); not saved.
    pub outfit_on: i32,
    /// S. Map markers, world x / z, -1 unused: 10 teleporter and 20 hint slots are
    /// used; the save stores 30 ints of each.
    pub tele_x: [i32; 30],
    pub tele_z: [i32; 30],
    pub hint_x: [i32; 30],
    pub hint_z: [i32; 30],
}

/// Everything the simulation mutates.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// S. 1..5 (`0x517f88`).
    pub island: i32,
    /// S. 0..2 (`0x41624`).
    pub difficulty: i32,
    /// S. 0 stay; 2..5 next island; 6 ending (`0x517f04`).
    pub next_island: i32,
    pub p: Player,
    pub shots: Shots,
    /// S. 210 slots.
    pub monsters: Vec<Monster>,
    /// S. Monster sprite tables of the current island (`0x414c0`).
    pub sprite_tables: [[i32; 11]; 5],
    /// S. `class_of[1..=4]` plus the byte that follows (`0x4159c..=0x415a0`).
    pub class_bytes: [u8; 5],
    /// Records in `MONSTERn.TRJ` (`0x414a8`).
    pub nmonsters: i32,
    /// Live monsters counted by the last update (`0x50c798`).
    pub alive_count: i32,
    /// S. 0 none, -1 spawn requested, 1 alive, 2 dead (`0x517ec8`).
    pub boss_state: i32,
    /// Island 5: boss lives left (`0x50c60c`).
    pub boss_respawns: i32,
    /// Auto-aim result of the last monster update (`0x50c7a0`, `0x50c79c`, `0x50c794`).
    pub autoaim_found: i32,
    pub autoaim_angle: f32,
    pub autoaim_best: f32,
    /// S. Pickup / decor list, 1600 slots, and its highest used index.
    pub bonus: Vec<Bonus>,
    pub bonus_last: i32,
    /// S. `{first, last}` frames per bonus kind.
    pub bonus_frames: [[i32; 2]; 100],
    /// S. Quest items and highest used slot.
    pub items: [Item; ITEMS],
    pub item_last: i32,
    pub q: Quest,
    /// S. Map cells that have been on screen, `[x][z]`, 0x10000 bytes (`0x2456a8`).
    pub explored: Vec<u8>,
    /// Origin cell of the 30x30 terrain window of the last rendered frame
    /// (`0x4f0ee0`, `0x4f0ee4`).
    pub win_x: i32,
    pub win_z: i32,
    /// Animated tile countdowns (`0x517da8`), 0..17, frame `(17 - t) / 3`.
    pub animap: [i32; 6],
    /// HUD displayed energy / money (`0x415fc`, `0x415f8`), chase the real values.
    pub hud_energy: i32,
    pub hud_money: i32,
    pub rng: Rng,
}

impl Default for Player {
    fn default() -> Self {
        Player {
            px: 0,
            pz: 0,
            py: 0,
            tx: 0,
            tz: 0,
            rx: 0,
            rz: 0,
            cam_x: 0,
            cam_y: 0,
            cam_z: 0,
            angle: 0.0,
            slide: 8,
            zone: 0,
            flying: 0,
            fuel: 0,
            hurt: 0,
            energy: 0,
            lives: 0,
            continues: 0,
            money: 0,
            dead: 0,
            dead_wait: 0,
            start_x: 0,
            start_z: 0,
            swim_timer: 0,
            drown: 0,
            walk: 0,
            strafe_ctr: 0,
            last_dir: 0,
            move_dir: 0,
            strafe: 0,
            moved: 0,
            bump: 0,
            shooting: 0,
            weapon: 0,
            power: 0,
            power_shots: 0,
            ammo: [0; 10],
            owned: [0; 10],
            fire_ctr: 0,
            last_weapon: -1,
            compass: 0,
            ticks: 1,
            wade_latch: 0,
            unk_517efc: 0,
            unk_517ebc: 0,
            unk_517f78: 0,
        }
    }
}

impl Default for Shots {
    fn default() -> Self {
        Shots {
            state: [0; SHOTS],
            x: [0; SHOTS],
            y: [0; SHOTS],
            z: [0; SHOTS],
            vx: [0; SHOTS],
            vz: [0; SHOTS],
            ty: [0; SHOTS],
            next: 0,
        }
    }
}

impl Default for Quest {
    fn default() -> Self {
        Quest {
            step: 0,
            step_shown: 0,
            busy: 0,
            touch: 0,
            touch_slot: 0,
            inv: [0; INVENTORY],
            ninv: 0,
            model: 0,
            model2: 0,
            boss_warned: 0,
            flags: IslandFlags::default(),
            outfit_on: 0,
            tele_x: [0; 30],
            tele_z: [0; 30],
            hint_x: [0; 30],
            hint_z: [0; 30],
        }
    }
}

impl Default for State {
    /// The state of a freshly started program (zeroed BSS plus initialised data).
    fn default() -> Self {
        State {
            island: 1,
            difficulty: 0,
            next_island: 0,
            p: Player::default(),
            shots: Shots::default(),
            monsters: vec![Monster::default(); MONSTERS],
            sprite_tables: [[0; 11]; 5],
            class_bytes: [0; 5],
            nmonsters: 0,
            alive_count: 0,
            boss_state: 0,
            boss_respawns: 0,
            autoaim_found: 0,
            autoaim_angle: 0.0,
            autoaim_best: 0.0,
            bonus: vec![Bonus::default(); BONUSES],
            bonus_last: 0,
            bonus_frames: tables::bonus_frames_init(),
            items: [Item::default(); ITEMS],
            item_last: 0,
            q: Quest::default(),
            explored: vec![0; 0x10000],
            win_x: 0,
            win_z: 0,
            animap: [0; 6],
            hud_energy: 0,
            hud_money: 0,
            rng: Rng::new(),
        }
    }
}

impl State {
    /// World position of the duck as of the last step: `(-rx, -rz)`.
    pub fn duck_world(&self) -> (i32, i32) {
        (self.p.rx.wrapping_neg(), self.p.rz.wrapping_neg())
    }

    /// `class_of[type]` (`0x4159b[type]`), type 1..=5.
    pub fn class_of(&self, ty: i32) -> u8 {
        self.class_bytes[((ty - 1).clamp(0, 4)) as usize]
    }

    /// `inv_has(id)` (`0x28d20`).
    pub fn inv_has(&self, id: i32) -> bool {
        let n = self.q.ninv.clamp(0, INVENTORY as i32) as usize;
        self.q.inv[..n].contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monster_words_round_trip() {
        let mut w = [0u32; 97];
        for (i, v) in w.iter_mut().enumerate() {
            *v = 0x1000_0000 + i as u32 * 7;
        }
        let m = Monster::from_words(&w);
        assert_eq!(m.to_words(), w);
        // Offsets from entities.md: shot_speed at 0x10c, path_idx at 0x15c.
        assert_eq!(m.shot_speed as u32, w[0x10c / 4]);
        assert_eq!(m.path_idx as u32, w[0x15c / 4]);
        assert_eq!(m.spread as u32, w[0x128 / 4]);
        assert_eq!(m.max_shots as u32, w[0x78 / 4]);
    }
}
