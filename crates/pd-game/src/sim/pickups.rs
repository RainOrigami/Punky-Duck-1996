//! Pickups / decor ("bonus" list), quest item table and their helpers; island start
//! functions (`0x17680`, `0x19030`, `0x1aa80`, `0x1c760`, `0x15ce0`).
//! Spec: `docs/re/game/entities.md` 4, `items-and-scripts.md` 2.

use super::state::{Bonus, Item, BONUSES, INVENTORY, ITEMS};
use super::tables::{self, Decor};
use super::{Bank, Event, Sim};
use pd_formats::trj::Trj;

impl Sim {
    /// `bonus_set(index, kind, x, y, z)` (`0x27f30`).
    pub(crate) fn bonus_set(&mut self, index: i32, kind: i32, x: i32, y: i32, z: i32) {
        if !(0..BONUSES as i32).contains(&index) {
            return;
        }
        self.s.bonus[index as usize] = Bonus { ty: kind, anim: 0, x, y, z };
        if self.s.bonus_last < index {
            self.s.bonus_last = index;
        }
    }

    /// `bonus_set(bonus_last++, ...)`, the way the start functions and the scripts
    /// append entries (`0x28cb5`, `0x17768`). `bonus_last` is the highest *used* index,
    /// so as coded the new entry overwrites the current last one and the counter then
    /// points one past it: the last `ILE0n.TRJ` record is lost under the first random
    /// money, and the list always ends in one never-written slot (all zero after a
    /// fresh start = an energy pill at the world origin).
    pub(crate) fn bonus_add(&mut self, kind: i32, x: i32, y: i32, z: i32) {
        if self.enh.fix_bonus_append_off_by_one {
            let i = self.s.bonus_last + 1;
            self.bonus_set(i, kind, x, y, z);
        } else {
            let i = self.s.bonus_last;
            self.s.bonus_last += 1;
            self.bonus_set(i, kind, x, y, z);
        }
    }

    /// `bonus_random_money` (`0x28bc0`): fill the list up to index 1470 with money on
    /// random cells whose 3x3 zone neighbourhood is all 0.
    fn bonus_random_money(&mut self) {
        let mut guard = 0u32;
        while self.s.bonus_last < 1470 {
            let (cx, cz) = loop {
                let cx = self.s.rng.next() % 256;
                let cz = self.s.rng.next() % 256;
                let i = cx * 256 + cz;
                let sum: u32 = [-257, -256, -255, -1, 0, 1, 255, 256, 257]
                    .iter()
                    .map(|d| self.terrain.zone_flat(i + d) as u32)
                    .sum();
                guard += 1;
                if sum == 0 || guard > 50_000_000 {
                    break (cx, cz);
                }
            };
            self.bonus_add(2, (cx - 128) * 300, -3000, (cz - 128) * 300);
        }
    }

    /// `item_add(id, x, y, z, icon, p6)` (`0x27ec0`): first free slot.
    pub(crate) fn item_add(&mut self, id: i32, x: i32, y: i32, z: i32, icon: i32, p6: i32) {
        let Some(slot) = self.s.items.iter().position(|it| it.id < 0) else {
            return;
        };
        self.s.items[slot] = Item { id, icon, p6, x, y, z };
        if self.s.item_last <= slot as i32 {
            self.s.item_last = slot as i32;
        }
    }

    /// Island start function: start position, `ILE0n.TRJ`, random money, extras, quest
    /// items. Does not clear the unused tail of the bonus list (as the original).
    pub(crate) fn island_start(&mut self, bonus_trj: &Trj) {
        let island = self.s.island;
        let s = &mut self.s;
        s.bonus_last = -1;
        s.item_last = -1;
        s.q.ninv = 0;
        s.q.step = -1;
        s.q.step_shown = -1;
        s.p.angle = 0.0;
        s.shots.next = 0;
        s.p.fire_ctr = 0;
        s.p.shooting = 0;
        s.boss_state = 0;
        s.q.model = -1;
        s.q.model2 = -1;
        if island == 5 {
            s.q.flags.vessel = [0; 4];
        }
        for i in 0..10 {
            s.q.tele_x[i] = -1;
            s.q.tele_z[i] = -1;
        }
        for i in 0..20 {
            s.q.hint_x[i] = -1;
            s.q.hint_z[i] = -1;
        }
        let (sx, sz) = tables::START[(island - 1) as usize];
        s.p.start_x = sx;
        s.p.unk_517ebc = 0;
        s.p.start_z = sz;
        s.p.px = sx;
        s.p.pz = sz;
        s.p.tx = sx;
        s.p.tz = sz;

        // bonus_load (0x27fd0): y is the constant -600.
        for (i, rec) in bonus_trj.records.iter().enumerate().take(BONUSES - 1) {
            self.bonus_set(i as i32, rec.kind, rec.x, -600, rec.z);
        }
        self.bonus_random_money();
        if let Some((kind, x, y, z)) = tables::island_extra_bonus(island) {
            self.bonus_add(kind, x, y, z);
        }
        self.s.explored.iter_mut().for_each(|b| *b = 0);
        for it in self.s.items.iter_mut() {
            it.id = -1;
        }
        for d in tables::island_items(island) {
            self.item_add(d.id, d.x, d.y, d.z, d.icon, d.p6);
        }
    }

    /// `apply_pickup(kind, index)` (`0x29d20`).
    fn apply_pickup(&mut self, kind: i32, index: usize) {
        let island = self.s.island;
        let p = &mut self.s.p;
        let sample = match kind {
            0 => {
                p.energy = 99;
                0
            }
            1 => {
                p.lives += 1;
                0
            }
            2 => {
                p.money += island * 100 + 100;
                14
            }
            3 => {
                p.continues += 1;
                0
            }
            4 => {
                p.fuel = 5000;
                0
            }
            5 => {
                p.swim_timer = 300;
                0
            }
            6 => {
                p.money += 10000;
                0
            }
            7 => {
                p.compass = 1;
                0
            }
            8 => {
                if p.power >= 2 {
                    return;
                }
                p.power = 1;
                p.power_shots = 400;
                23
            }
            9 => {
                p.power = 2;
                p.power_shots = 400;
                23
            }
            10 | 12 | 14 | 16 | 18 | 20 | 22 => {
                p.owned[((kind - 8) / 2) as usize] = 1;
                23
            }
            11 | 13 | 15 | 17 | 19 | 21 | 23 => {
                let w = ((kind - 9) / 2) as usize;
                p.ammo[w] += [0, 200, 150, 150, 150, 100, 100, 8][w];
                23
            }
            _ => return,
        };
        self.s.bonus[index].ty = -1;
        self.sfx(sample, 255);
    }

    /// `pickups_update` (per-island copies `0x18160`..): animate, collide, queue
    /// sprites; then the death trigger.
    pub(crate) fn pickups_update(&mut self) {
        let island = self.s.island;
        let last = self.s.bonus_last.min(BONUSES as i32 - 1);
        for i in 0..=last.max(-1) {
            let i = i as usize;
            let b = self.s.bonus[i];
            let kind = b.ty;
            if kind < 0 {
                continue;
            }
            let [first, lastf] = self.s.bonus_frames[(kind as usize).min(99)];
            let mut frame = first;
            if first < lastf {
                let a = (b.anim + 1) % ((lastf - first) * 2);
                self.s.bonus[i].anim = a;
                frame = first + a / 2;
            }
            let dx = b.x.wrapping_neg().wrapping_sub(self.s.p.px).wrapping_abs();
            let dz = b.z.wrapping_neg().wrapping_sub(self.s.p.pz).wrapping_abs();
            // The original tests the X distance twice; Z is not tested for drawing.
            if dx >= 4500 {
                continue;
            }
            if dz < 120 && dx < 120 && self.s.p.hurt == 0 {
                self.apply_pickup(kind, i);
                let flying = self.s.p.flying == 1;
                match tables::decor_class(island, kind) {
                    Decor::Solid => {
                        if flying {
                            self.s.p.bump = 2;
                        } else {
                            self.s.p.bump = 3;
                            self.sfx(21, 255);
                        }
                    }
                    Decor::Harm => {
                        self.s.p.energy -= 10;
                        self.s.p.hurt = 20;
                    }
                    Decor::HarmOnFoot => {
                        if self.s.p.flying == 0 {
                            self.s.p.energy -= 10;
                            self.s.p.hurt = 20;
                        }
                    }
                    Decor::None => {}
                }
            }
            self.sprite(frame, Bank::Bonus, 0, b.x, b.y, b.z, 4.0);
        }
        let p = &mut self.s.p;
        if p.energy < 1 && p.dead == 0 {
            p.dead_wait = 0;
            p.dead = 35;
            self.events.push(Event::CdTrack(6));
        }
    }

    /// `items_update` (island 1 = `0x184a0`): queue item sprites and compute `touch`.
    pub(crate) fn items_update(&mut self) {
        self.s.q.touch = -1;
        let last = self.s.item_last.min(ITEMS as i32 - 1);
        for slot in 0..=last.max(-1) {
            let it = self.s.items[slot as usize];
            if it.id < 0 {
                continue;
            }
            let dx = it.x.wrapping_neg().wrapping_sub(self.s.p.px) as f64;
            let dz = it.z.wrapping_neg().wrapping_sub(self.s.p.pz) as f64;
            // Signed compare on X only.
            if dx < 4500.0 {
                self.sprite(it.id, Bank::Icone, 0, it.x, it.y, it.z, 4.0);
                if dx * dx + dz * dz < 24000.0 && it.icon < 100 {
                    self.s.q.touch = it.id;
                    self.s.q.touch_slot = slot;
                }
            }
        }
        if self.s.q.touch == -1 {
            self.s.q.busy = 0;
        }
    }

    /// `hint(id)` (`0x27e50`): remember the item position in the first free hint
    /// marker (20 slots).
    pub(crate) fn hint(&mut self, id: i32) {
        let Some(j) = self.s.q.hint_x[..20].iter().position(|&v| v == -1) else {
            return;
        };
        for slot in 0..=self.s.item_last.clamp(-1, ITEMS as i32 - 1) {
            let it = self.s.items[slot as usize];
            if it.id == id {
                self.s.q.hint_x[j] = it.x;
                self.s.q.hint_z[j] = it.z;
            }
        }
    }

    /// `mark(id)` (`0x27de0`): same for the 10 teleporter markers.
    pub(crate) fn mark(&mut self, id: i32) {
        let Some(j) = self.s.q.tele_x[..10].iter().position(|&v| v == -1) else {
            return;
        };
        for slot in 0..=self.s.item_last.clamp(-1, ITEMS as i32 - 1) {
            let it = self.s.items[slot as usize];
            if it.id == id {
                self.s.q.tele_x[j] = it.x;
                self.s.q.tele_z[j] = it.z;
            }
        }
    }

    /// `inv_remove(id, drop)` (`0x286b0`). Returns whether the item was held.
    pub(crate) fn inv_remove(&mut self, id: i32, drop: bool) -> bool {
        let n = self.s.q.ninv.clamp(0, INVENTORY as i32) as usize;
        let Some(idx) = self.s.q.inv[..n].iter().rposition(|&v| v == id) else {
            return false;
        };
        self.sfx(25, 255);
        // The copy loop reads one entry past the count (inv[20] is `flag70`).
        for i in idx..n {
            self.s.q.inv[i] = if i + 1 < INVENTORY { self.s.q.inv[i + 1] } else { self.s.q.flags.flag70 };
        }
        self.s.q.ninv -= 1;
        if drop {
            let (x, z) = (self.s.p.px.wrapping_neg(), self.s.p.pz.wrapping_neg());
            self.item_add(id, x, -300, z, 100, 0);
        }
        true
    }

    /// Push an id on the inventory (`inv[ninv++] = id`).
    pub(crate) fn inv_push(&mut self, id: i32) {
        let n = self.s.q.ninv;
        if (0..INVENTORY as i32).contains(&n) {
            self.s.q.inv[n as usize] = id;
        }
        self.s.q.ninv = n + 1;
    }

    /// Replace `old` by `new` in the globals that lie on the 24-byte grid of the item
    /// table beyond its 30 slots. `item_rename` and `item_remove_all` use the *bonus*
    /// counter as their loop bound, so in the original they scan about 1470 "slots",
    /// far past the table, and patch any aligned i32 that happens to equal `old`.
    /// Modelled for the gameplay globals on that grid (addresses `0x516ae8 + 24 k`).
    fn item_scan_overflow(&mut self, old: i32, new: i32, bound: i32) {
        let hit = |k: i32, v: &mut i32| {
            if k < bound && *v == old {
                *v = new;
            }
        };
        let s = &mut self.s;
        for j in 0..42 {
            hit(30 + j, &mut s.shots.vz[(j * 6) as usize]); // 0x516db8
        }
        for j in 0..41 {
            hit(114 + j, &mut s.shots.vx[(4 + j * 6) as usize]); // 0x517588
        }
        hit(199, &mut s.p.ammo[4]);
        hit(200, &mut s.animap[0]);
        hit(205, &mut s.q.inv[0]);
        hit(206, &mut s.q.inv[6]);
        hit(207, &mut s.q.inv[12]);
        hit(208, &mut s.q.inv[18]);
        hit(214, &mut s.p.power_shots);
        hit(216, &mut s.q.touch); // 0x517f28
    }

    /// `item_rename(old, new)` (`0x28b70`): scans slots `0 .. bonus_last - 1`.
    pub(crate) fn item_rename(&mut self, old: i32, new: i32) {
        let bound = self.s.bonus_last;
        for (k, it) in self.s.items.iter_mut().enumerate() {
            if (k as i32) < bound && it.id == old {
                it.id = new;
            }
        }
        if !self.enh.fix_item_scan_overflow {
            self.item_scan_overflow(old, new, bound);
        }
    }

    /// `item_remove_all(id)` (`0x28670`): scans slots `0 ..= bonus_last`.
    pub(crate) fn item_remove_all(&mut self, id: i32) {
        let bound = self.s.bonus_last.saturating_add(1);
        for (k, it) in self.s.items.iter_mut().enumerate() {
            if (k as i32) < bound && it.id == id {
                it.id = -1;
            }
        }
        if !self.enh.fix_item_scan_overflow {
            self.item_scan_overflow(id, -1, bound);
        }
    }
}
