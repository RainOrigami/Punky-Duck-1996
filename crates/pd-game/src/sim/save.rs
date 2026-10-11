//! The original save-game layout (`SAVEGAME.__n`, 181 817 bytes), read into and written
//! from [`State`]. Order and sizes follow the `fread` calls of `state_load` (`0x293c0`)
//! and `0x26050`; see `docs/re/game/save-and-settings.md` section 2.
//!
//! Not in a save (kept from the running program by the original): player shot
//! positions and states, `rx/rz`, animation counters, the random seed, `alive_count`,
//! `boss_respawns`, `outfit_on`, the zone map patches (scripts re-apply them).

use super::state::*;

/// Size of a save file.
pub const SAVE_SIZE: usize = 181_817;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError(pub String);

impl core::fmt::Display for SaveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "save game: {}", self.0)
    }
}

impl std::error::Error for SaveError {}

struct Rd<'a> {
    d: &'a [u8],
    o: usize,
}

impl Rd<'_> {
    fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes([self.d[self.o], self.d[self.o + 1], self.d[self.o + 2], self.d[self.o + 3]]);
        self.o += 4;
        v
    }
    fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    fn ints(&mut self, out: &mut [i32]) {
        for v in out {
            *v = self.i32();
        }
    }
}

struct Wr {
    d: Vec<u8>,
}

impl Wr {
    fn u32(&mut self, v: u32) {
        self.d.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    fn ints(&mut self, v: &[i32]) {
        for &x in v {
            self.i32(x);
        }
    }
}

impl State {
    /// Overwrite the saved part of the state from a save file. Everything else keeps
    /// its current value, as in the original.
    pub fn read_save(&mut self, data: &[u8]) -> Result<(), SaveError> {
        if data.len() != SAVE_SIZE {
            return Err(SaveError(format!("{} bytes, expected {SAVE_SIZE}", data.len())));
        }
        let mut r = Rd { d: data, o: 0 };
        let island = r.i32();
        if !(1..=5).contains(&island) {
            return Err(SaveError(format!("island {island}")));
        }
        self.island = island;
        self.p.angle = f32::from_bits(r.u32());
        self.q.step = r.i32();
        self.q.step_shown = r.i32();
        self.q.busy = r.i32();
        self.p.unk_517f78 = r.i32();
        self.q.model = r.i32();
        self.q.model2 = r.i32();
        r.ints(&mut self.q.inv);
        self.p.px = r.i32();
        self.p.py = r.i32();
        self.p.pz = r.i32();
        self.p.tx = r.i32();
        self.p.unk_517efc = r.i32();
        self.p.tz = r.i32();
        self.q.boss_warned = r.i32();
        self.q.ninv = r.i32();
        r.ints(&mut self.shots.ty[..50]);
        self.p.weapon = r.i32();
        r.ints(&mut self.p.ammo);
        r.ints(&mut self.p.owned);
        self.p.shooting = r.i32();
        self.p.flying = r.i32();
        self.p.power = r.i32();
        self.p.power_shots = r.i32();
        self.p.fuel = r.i32();
        self.p.hurt = r.i32();
        self.p.money = r.i32();
        self.p.continues = r.i32();
        self.p.lives = r.i32();
        self.p.energy = r.i32();
        self.p.swim_timer = r.i32();
        self.next_island = r.i32();
        self.q.touch = r.i32();
        self.q.touch_slot = r.i32();
        self.p.dead = r.i32();
        self.p.start_x = r.i32();
        self.p.unk_517ebc = r.i32();
        self.p.start_z = r.i32();
        self.p.dead_wait = r.i32();
        self.p.bump = r.i32();
        self.bonus_last = r.i32();
        self.item_last = r.i32();
        for e in self.bonus_frames.iter_mut() {
            r.ints(e);
        }
        for b in self.bonus.iter_mut() {
            *b = Bonus { ty: r.i32(), anim: r.i32(), x: r.i32(), y: r.i32(), z: r.i32() };
        }
        for it in self.items.iter_mut() {
            *it = Item { id: r.i32(), icon: r.i32(), p6: r.i32(), x: r.i32(), y: r.i32(), z: r.i32() };
        }
        self.p.compass = r.i32();
        self.boss_state = r.i32();
        r.ints(&mut self.q.tele_x);
        r.ints(&mut self.q.tele_z);
        r.ints(&mut self.q.hint_x);
        r.ints(&mut self.q.hint_z);
        self.explored.copy_from_slice(&data[r.o..r.o + 0x10000]);
        r.o += 0x10000;
        let f = &mut self.q.flags;
        f.site_seen = r.i32();
        f.flag90 = r.i32();
        f.unk_e74 = r.i32();
        f.flag70 = r.i32();
        f.unk_e94 = r.i32();
        f.unk_e98 = r.i32();
        f.unk_ea0 = r.i32();
        f.unk_e8c = r.i32();
        r.ints(&mut f.vessel);
        for m in self.monsters.iter_mut() {
            let mut w = [0u32; 97];
            for v in w.iter_mut() {
                *v = r.u32();
            }
            *m = Monster::from_words(&w);
        }
        for t in self.sprite_tables.iter_mut() {
            r.ints(t);
        }
        self.class_bytes.copy_from_slice(&data[r.o..r.o + 5]);
        r.o += 5;
        self.difficulty = r.i32();
        debug_assert_eq!(r.o, SAVE_SIZE);
        Ok(())
    }

    /// Serialise in the original layout.
    pub fn write_save(&self) -> Vec<u8> {
        let mut w = Wr { d: Vec::with_capacity(SAVE_SIZE) };
        w.i32(self.island);
        w.u32(self.p.angle.to_bits());
        w.i32(self.q.step);
        w.i32(self.q.step_shown);
        w.i32(self.q.busy);
        w.i32(self.p.unk_517f78);
        w.i32(self.q.model);
        w.i32(self.q.model2);
        w.ints(&self.q.inv);
        w.ints(&[
            self.p.px,
            self.p.py,
            self.p.pz,
            self.p.tx,
            self.p.unk_517efc,
            self.p.tz,
            self.q.boss_warned,
            self.q.ninv,
        ]);
        w.ints(&self.shots.ty[..50]);
        w.i32(self.p.weapon);
        w.ints(&self.p.ammo);
        w.ints(&self.p.owned);
        w.ints(&[
            self.p.shooting,
            self.p.flying,
            self.p.power,
            self.p.power_shots,
            self.p.fuel,
            self.p.hurt,
            self.p.money,
            self.p.continues,
            self.p.lives,
            self.p.energy,
            self.p.swim_timer,
            self.next_island,
            self.q.touch,
            self.q.touch_slot,
            self.p.dead,
            self.p.start_x,
            self.p.unk_517ebc,
            self.p.start_z,
            self.p.dead_wait,
            self.p.bump,
            self.bonus_last,
            self.item_last,
        ]);
        for e in &self.bonus_frames {
            w.ints(e);
        }
        for b in &self.bonus {
            w.ints(&[b.ty, b.anim, b.x, b.y, b.z]);
        }
        for it in &self.items {
            w.ints(&[it.id, it.icon, it.p6, it.x, it.y, it.z]);
        }
        w.i32(self.p.compass);
        w.i32(self.boss_state);
        w.ints(&self.q.tele_x);
        w.ints(&self.q.tele_z);
        w.ints(&self.q.hint_x);
        w.ints(&self.q.hint_z);
        w.d.extend_from_slice(&self.explored);
        let f = &self.q.flags;
        w.ints(&[
            f.site_seen, f.flag90, f.unk_e74, f.flag70, f.unk_e94, f.unk_e98, f.unk_ea0, f.unk_e8c,
        ]);
        w.ints(&f.vessel);
        for m in &self.monsters {
            for v in m.to_words() {
                w.u32(v);
            }
        }
        for t in &self.sprite_tables {
            w.ints(t);
        }
        w.d.extend_from_slice(&self.class_bytes);
        w.i32(self.difficulty);
        debug_assert_eq!(w.d.len(), SAVE_SIZE);
        w.d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_round_trips() {
        let mut s = State::default();
        s.p.px = -19650;
        s.p.angle = -1.25;
        s.monsters[137].shot_vx[3] = -77;
        s.q.flags.vessel = [1, 0, 1, 0];
        s.class_bytes = [0, 1, 0, 0, 1];
        let bytes = s.write_save();
        assert_eq!(bytes.len(), SAVE_SIZE);
        let mut t = State::default();
        t.read_save(&bytes).unwrap();
        assert_eq!(t.write_save(), bytes);
        assert_eq!(t.p.px, -19650);
        assert_eq!(t.monsters[137].shot_vx[3], -77);
        // Offsets from save-and-settings.md.
        assert_eq!(&bytes[112..116], &(-19650i32).to_le_bytes());
        assert_eq!(bytes[181_808 + 1], 1);
    }

    #[test]
    fn rejects_wrong_size() {
        assert!(State::default().read_save(&[0; 100]).is_err());
    }
}
