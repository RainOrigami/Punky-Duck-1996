//! The five island quest scripts (`0x178d0`, `0x19300`, `0x1ae00`, `0x1cae0`, `0x160d0`),
//! one hand-written function per island in the original, run once per frame.
//! Spec: `docs/re/game/items-and-scripts.md`. Island 1 follows the decompilation line
//! by line; islands 2..5 are transcribed from the spec.
//!
//! Modal calls (`say`, `picture`, ...) become [`Event`]s; the script goes on in the same
//! frame exactly as the original does after the modal screen closes.

use super::{chop, Event, Sim};

const YAW: f32 = -1.5708;

impl Sim {
    fn say(&mut self, line: i32) {
        self.events.push(Event::Say(line));
    }

    fn picture(&mut self, name: &'static str) {
        self.events.push(Event::Picture(name));
    }

    /// Remove the touched item from the world and put `id` in the inventory.
    fn take_as(&mut self, id: i32) {
        let slot = self.s.q.touch_slot;
        if (0..30).contains(&slot) {
            self.s.items[slot as usize].id = -1;
        }
        self.inv_push(id);
    }

    fn take(&mut self) {
        let id = self.s.q.touch;
        self.take_as(id);
    }

    fn refuse(&mut self, line: i32, id: i32) {
        self.say(line);
        self.hint(id);
        self.s.q.busy = 1;
    }

    /// `PICK(id, lo..=hi, ok_line, refuse_line)`.
    fn pick(&mut self, id: i32, lo: i32, hi: i32, ok: i32, no: i32) {
        if self.s.q.touch != id {
            return;
        }
        if (lo..=hi).contains(&self.s.q.step) {
            self.say(ok);
            self.take();
            self.s.q.step += 1;
        } else {
            self.refuse(no, id);
        }
    }

    fn teleport_to(&mut self, x: i32, z: i32) {
        let p = &mut self.s.p;
        p.tx = x;
        p.tz = z;
        p.px = x;
        p.pz = z;
        self.sfx(22, 255);
    }

    /// First contact with pad `a`: it becomes `to`, the 20-frame stun doubles as a
    /// delay, the marker is added for `marked`.
    fn arm_pad(&mut self, a: i32, to: i32, marked: i32) {
        if self.s.q.touch == a {
            self.item_rename(a, to);
            self.s.p.hurt = 20;
            self.mark(marked);
        }
    }

    fn pad_jump(&mut self, active: i32, dest: (i32, i32)) {
        if self.s.q.touch == active && self.s.p.hurt == 0 {
            self.teleport_to(dest.0, dest.1);
        }
    }

    /// The scripts arm all pads first and then test all active pads.
    fn teleporters(&mut self, pads: &[(i32, (i32, i32))]) {
        for &(a, _) in pads {
            self.arm_pad(a, a + 1, a + 1);
        }
        for &(a, dest) in pads {
            self.pad_jump(a + 1, dest);
        }
    }

    /// Move the duck straight backwards out of a solid trigger zone
    /// (`px = trunc(cos * 28 + px)`, ...; `tx/tz` are not touched).
    pub(crate) fn eject_from(&mut self, zone: i32) {
        let p = &mut self.s.p;
        let a = p.angle as f64;
        let mut guard = 0;
        while self.terrain.zone_at(p.px.wrapping_neg(), p.pz.wrapping_neg()) == zone && guard < 100_000 {
            p.px = chop(a.cos() * 28.0 + p.px as f64);
            p.pz = chop(28.0 * a.sin() + p.pz as f64);
            guard += 1;
        }
    }

    /// Boss / exit tail shared by islands 1..4. `s1` = step while the boss lives.
    fn finale(&mut self, s1: i32, site: i32, warn: i32, easy: i32, medium: i32, hard: i32) -> bool {
        let diff = self.s.difficulty;
        if self.s.q.boss_warned == 0 && self.s.q.step == s1 && self.s.p.zone == site && self.s.boss_state == 1 {
            self.say(warn);
            self.s.q.boss_warned = 1;
        }
        if self.s.q.step == s1 && self.s.boss_state == 2 {
            match diff {
                0 => self.say(easy),
                1 => self.say(medium),
                2 => self.say(hard),
                _ => {}
            }
            if (0..=2).contains(&diff) {
                self.s.q.step += 1;
            }
        }
        if self.s.q.step == s1 + 1 {
            if diff == 0 {
                self.s.q.step = s1 + 2;
            }
            if diff == 1 && self.s.alive_count < self.s.nmonsters / 2 {
                self.s.q.step += 1;
                self.say(easy);
            }
            if diff == 2 && self.s.alive_count < 2 {
                self.s.q.step += 1;
                self.say(easy);
            }
        }
        self.s.q.step == s1 + 2 && self.s.p.zone == site
    }

    fn jingle(&mut self) {
        if self.s.q.step_shown != self.s.q.step {
            self.sfx(27, 255);
            self.s.q.step_shown = self.s.q.step;
        }
    }

    pub(crate) fn run_script(&mut self) {
        match self.s.island {
            1 => self.script_1(),
            2 => self.script_2(),
            3 => self.script_3(),
            4 => self.script_4(),
            5 => self.script_5(),
            _ => {}
        }
    }

    /// Island 1, the raft (`0x178d0`).
    fn script_1(&mut self) {
        self.teleporters(&[(15, (20700, 22200)), (17, (12225, -17100))]);
        if self.s.q.model != -1 {
            let m = self.s.q.model;
            self.mesh(6, m, 29400.0, 0.0, -17550.0, YAW);
        }
        if self.s.q.busy >= 1 {
            return;
        }
        if self.input.key1 && self.s.q.step > -1 {
            self.picture("RADEAU.RON");
        }
        if self.s.p.zone == 7 && self.s.q.flags.site_seen == 0 {
            self.say(6);
            self.s.q.flags.site_seen = 1;
        }
        if self.s.q.touch == 0 {
            self.picture("RADEAU.RON");
            self.take_as(0);
            self.s.q.step += 1;
        }
        self.pick(2, 0, 1, 0, 4);
        self.pick(1, 0, 1, 1, 4);
        for (id, ok) in [(4, 2), (3, 3)] {
            if self.s.q.touch == id {
                let step = self.s.q.step;
                if (0..=1).contains(&step) {
                    self.refuse(7, id);
                } else if !(2..=3).contains(&step) {
                    self.refuse(4, id);
                } else if self.s.q.flags.site_seen == 1 {
                    self.say(ok);
                    self.take_as(id);
                    self.s.q.step += 1;
                } else {
                    self.refuse(5, id);
                }
            }
        }
        if self.s.q.step == 4 && self.s.p.zone == 7 {
            self.s.q.step = 5;
            self.inv_remove(3, false);
            self.inv_remove(4, false);
            self.say(10);
            self.s.q.model = 2;
        }
        if self.s.q.step > 4 {
            // The raft footprint becomes the raised zone 17.
            self.terrain.zone_fill(0xe1, 0x45, 0xe3, 0x46, 17);
        }
        if self.s.q.step == 6 && self.s.p.zone == 7 {
            self.s.q.step = 7;
            self.inv_remove(5, false);
            self.say(11);
            self.s.q.model = 1;
        }
        for (id, hi, at, ok) in [(5, 4, 5, 9), (6, 5, 7, 12)] {
            if self.s.q.touch == id {
                let step = self.s.q.step;
                if (2..=hi).contains(&step) {
                    self.refuse(8, id);
                } else if step < 5 {
                    self.refuse(7, id);
                } else if step == at {
                    self.say(ok);
                    self.take_as(id);
                    self.s.q.step += 1;
                }
            }
        }
        if self.s.q.step == 8 && self.s.p.zone == 7 {
            self.s.q.step = 9;
            self.inv_remove(6, false);
            self.say(13);
            self.s.boss_state = -1;
            self.s.q.model = 0;
        }
        if self.finale(9, 7, 14, 15, 16, 17) {
            self.s.next_island = 2;
        }
        self.jingle();
    }

    /// Island 2, the ULM (`0x19300`).
    fn script_2(&mut self) {
        self.teleporters(&[(13, (30281, -22912)), (15, (-14737, 16790)), (17, (-6093, -24787))]);
        if self.s.q.model != -1 {
            let m = self.s.q.model;
            self.mesh(0, m, -5862.0, 0.0, 5163.0, YAW);
        }
        if self.s.q.busy >= 1 {
            return;
        }
        if self.s.p.zone == 7 && self.s.q.flags.flag90 == 0 {
            self.bonus_add(44, -5923, 0, 4088);
            self.s.q.flags.flag90 = 1;
            self.say(8);
        }
        if self.input.key1 && self.s.q.step > -1 {
            self.picture("ULM.RON");
        }
        if self.s.q.touch == 0 {
            self.sfx(0, 255);
            self.picture("ULM.RON");
            self.take_as(0);
            self.s.q.step += 1;
        }
        if self.s.q.touch == 1 {
            if self.s.q.step == 0 {
                self.say(0);
                self.take_as(1);
                self.s.q.step = 1;
            } else {
                self.refuse(1, 1);
            }
        }
        self.pick(2, 1, 3, 2, 3);
        self.pick(4, 1, 3, 4, 5);
        if self.s.q.flags.flag70 != 0 {
            self.terrain.zone_fill(0x0d, 0xda, 0x11, 0xdd, 0);
        }
        if self.s.p.zone == 10 && self.s.q.flags.flag70 == 0 {
            if (1..=3).contains(&self.s.q.step) {
                self.say(6);
                self.s.q.flags.flag70 = 1;
                self.s.q.step += 1;
                self.inv_push(3);
            } else {
                let t = self.s.q.touch;
                self.refuse(7, t);
            }
        }
        if self.s.p.zone == 7 && self.s.q.step == 4 {
            self.say(9);
            self.inv_remove(4, false);
            self.inv_remove(3, false);
            self.inv_remove(2, false);
            self.s.q.step = 5;
            self.s.q.model = 0;
        }
        if self.s.q.step > 4 {
            self.terrain.zone_fill(0x6b, 0x8d, 0x70, 0x91, 20);
            self.eject_from(20);
        }
        if self.s.q.touch == 5 {
            let step = self.s.q.step;
            if (5..=7).contains(&step) {
                self.say(10);
                self.take_as(5);
                self.s.q.step += 1;
            } else if step == 4 {
                self.refuse(11, 5);
            } else {
                self.refuse(12, 5);
            }
        }
        self.pick(6, 5, 7, 13, 14);
        self.pick(7, 5, 7, 15, 16);
        if self.s.p.zone == 20 && self.s.q.step == 8 {
            self.say(17);
            self.inv_remove(7, false);
            self.inv_remove(6, false);
            self.s.q.step = 9;
            self.s.q.model = 1;
        }
        self.pick(8, 9, 9, 18, 19);
        if self.s.p.zone == 20 && self.s.q.step == 10 {
            self.say(20);
            self.inv_remove(8, false);
            self.s.q.step = 11;
            self.s.q.model = 2;
        }
        if self.s.q.touch == 9 {
            if self.s.q.step == 11 {
                self.say(21);
                self.take_as(12);
                self.s.q.step = 12;
            } else {
                self.refuse(22, 9);
            }
        }
        if self.s.p.zone == 20 && self.s.q.step == 12 {
            self.say(23);
            self.inv_remove(12, false);
            self.s.q.step = 13;
            self.s.q.model = 3;
        }
        self.pick(10, 13, 13, 24, 25);
        if self.s.p.zone == 20 && self.s.q.step == 14 {
            self.say(26);
            self.inv_remove(10, false);
            self.s.q.step = 15;
            self.s.q.model = 4;
        }
        self.pick(11, 15, 15, 27, 28);
        if self.s.p.zone == 20 && self.s.q.step == 16 {
            self.say(29);
            self.inv_remove(11, false);
            self.s.q.step = 17;
            self.s.q.model = 5;
        }
        if self.s.p.zone == 0 && self.s.q.step == 17 {
            self.say(30);
            self.s.q.step = 18;
            self.s.boss_state = -1;
        }
        if self.finale(18, 20, 31, 32, 33, 34) {
            self.s.next_island = 3;
        }
        self.jingle();
    }

    /// The miscoded fifth pad of islands 3 and 4: `item_rename(a, a - 1)` and
    /// `mark(a + 1)`, so the pad turns into a second copy of the previous active pad
    /// (and sends the duck to *that* pad's destination); its own branch is dead.
    fn fifth_pad(&mut self, a: i32) {
        if self.enh.fix_fifth_teleporter {
            self.arm_pad(a, a + 1, a + 1);
        } else {
            self.arm_pad(a, a - 1, a + 1);
        }
    }

    /// Island 3: shelter, radar, jetski (`0x1ae00`).
    fn script_3(&mut self) {
        let pads = [
            (14, (-18525, -29175)),
            (16, (27046, -17709)),
            (18, (-2981, 8409)),
            (20, (-25753, -5896)),
        ];
        for &(a, _) in &pads {
            self.arm_pad(a, a + 1, a + 1);
        }
        self.fifth_pad(22);
        for &(a, dest) in &pads {
            self.pad_jump(a + 1, dest);
        }
        self.pad_jump(23, (-26081, 17596));
        if self.s.q.model != -1 {
            let m = self.s.q.model;
            self.mesh(0, m, 12900.0, 0.0, -16800.0, 0.0);
        }
        if self.s.q.model2 != -1 {
            let m = self.s.q.model2;
            self.mesh(1, m, 13050.0, 0.0, -15900.0, 0.0);
        }
        if self.input.key1 && self.s.q.step > -1 {
            self.picture("RADAR.RON");
        }
        if self.input.key2 && self.s.inv_has(13) {
            self.picture("JETSKI.RON");
        }
        if self.s.q.busy >= 1 {
            return;
        }
        if self.s.q.touch == 12 {
            self.picture("RADAR.RON");
            self.take_as(12);
            self.s.q.step += 1;
        }
        if self.s.q.touch == 13 {
            self.picture("JETSKI.RON");
            if self.s.q.step == 6 {
                self.s.q.step = 7;
            }
            self.take_as(13);
        }
        self.pick(5, 0, 0, 0, 1);
        self.pick(3, 1, 1, 2, 3);
        if self.s.p.zone == 15 && self.s.q.step == 2 {
            self.say(4);
            self.inv_remove(5, false);
            self.s.q.step = 3;
            self.s.q.model = 4;
        }
        if self.s.q.step > 2 {
            self.terrain.zone_fill(0xaa, 0x47, 0xac, 0x4b, 14);
            self.eject_from(14);
        }
        for (id, ok, no) in [(8, 5, 6), (0, 7, 8)] {
            if self.s.q.touch == id {
                let step = self.s.q.step;
                if step == 2 {
                    self.refuse(13, id);
                } else if (3..=4).contains(&step) {
                    self.say(ok);
                    self.take_as(id);
                    self.s.q.step += 1;
                } else {
                    self.refuse(no, id);
                }
            }
        }
        if self.s.q.step == 5 && self.s.p.zone == 14 {
            self.s.q.step = 6;
            self.bonus_add(43, 13050, -300, -16200);
            self.inv_remove(0, false);
            self.inv_remove(8, false);
            self.say(18);
        }
        if self.s.q.step == 6 && self.s.inv_has(13) {
            self.s.q.step = 7;
        }
        self.pick(4, 7, 9, 11, 12);
        self.pick(9, 7, 9, 14, 15);
        if self.s.p.zone == 10 && self.s.q.step < 10 {
            // The marmot.
            if !self.s.inv_has(9) {
                self.events.push(Event::TextOnPicture(16, "MISS.RON"));
            } else {
                self.inv_push(6);
                self.s.q.step += 1;
                self.events.push(Event::TextOnPicture(17, "CATCH.RON"));
                self.inv_remove(9, true);
            }
        }
        if self.s.p.zone == 14 && self.s.q.step == 10 {
            self.s.q.step = 11;
            self.bonus_add(44, 13050, -300, -16200);
            self.inv_remove(6, false);
            self.inv_remove(4, false);
            self.say(19);
        }
        if self.s.q.step == 11 && self.s.p.zone == 0 {
            self.say(20);
            self.s.q.step = 12;
        }
        for (id, at, ok, no, built, line, model2) in [
            (10, 12, 21, 22, 13, 35, 0),
            (1, 14, 23, 24, 15, 36, 1),
            (2, 16, 25, 26, 17, 30, 2),
            (11, 18, 27, 28, 19, 29, 3),
        ] {
            self.pick(id, at, at, ok, no);
            if self.s.q.step == built && self.s.p.zone == 14 {
                self.s.q.step = built + 1;
                self.say(line);
                self.inv_remove(id, false);
                self.s.q.model2 = model2;
            }
        }
        self.pick(7, 20, 20, 31, 32);
        if self.s.q.step == 21 {
            self.say(37);
            self.s.q.step = 22;
            self.s.boss_state = -1;
        }
        if self.finale(22, 14, 38, 39, 40, 41) {
            self.s.next_island = 4;
        }
        self.jingle();
    }

    /// Island 4: tower, runway, glider (`0x1cae0`).
    fn script_4(&mut self) {
        let pads = [
            (16, (14475, -29025)),
            (18, (5250, -25425)),
            (20, (21412, -20437)),
            (22, (-25368, -5756)),
        ];
        for &(a, _) in &pads {
            self.arm_pad(a, a + 1, a + 1);
        }
        self.fifth_pad(24);
        for &(a, dest) in &pads {
            self.pad_jump(a + 1, dest);
        }
        self.pad_jump(25, (1687, 7012));
        if self.s.q.model != -1 {
            let m = self.s.q.model;
            self.mesh(0, m, 5380.0, 0.0, -15600.0, 0.0);
        }
        if self.s.q.model2 != -1 {
            let m = self.s.q.model2;
            let y = self.terrain.height_at(4180, -18400) as f32;
            self.mesh(1, m, 4180.0, y, -18400.0, 0.0);
        }
        if self.input.key1 && self.s.inv_has(15) {
            self.picture("TOUR.RON");
        }
        if self.input.key2 && self.s.inv_has(14) {
            self.picture("BIPLAN.RON");
        }
        if self.s.q.busy >= 1 {
            return;
        }
        if self.s.q.touch == 15 {
            self.picture("TOUR.RON");
            self.take_as(15);
            self.s.q.step += 1;
        }
        if self.s.q.touch == 14 {
            self.picture("BIPLAN.RON");
            self.take_as(14);
        }
        self.pick(8, 0, 1, 0, 1);
        self.pick(9, 0, 1, 2, 3);
        self.pick(6, 2, 2, 4, 5);
        if self.s.p.zone == 15 && self.s.q.step == 3 {
            self.s.q.model = 1;
            self.s.q.step = 4;
            self.say(6);
            self.inv_remove(6, false);
            self.inv_remove(9, false);
        }
        if self.s.q.step > 3 {
            self.terrain.zone_fill(0x91, 0x4a, 0x93, 0x4c, 10);
            self.eject_from(10);
        }
        self.pick(10, 4, 4, 7, 8);
        if self.s.p.zone == 10 && self.s.q.step == 5 {
            self.s.q.step = 6;
            self.inv_remove(10, false);
            self.item_remove_all(10);
            self.item_add(10, 5380, -51000, -15600, 10, 0);
            self.say(9);
        }
        if self.s.p.zone == 16 && self.s.q.step == 6 {
            self.s.q.step = 7;
            self.say(10);
        }
        self.pick(4, 7, 7, 11, 12);
        self.pick(11, 8, 8, 13, 14);
        self.pick(5, 9, 9, 15, 16);
        if self.s.p.zone == 16 && self.s.q.step == 10 {
            self.inv_remove(4, true);
            self.inv_remove(11, false);
            self.inv_remove(5, true);
            self.s.q.step = 11;
            self.say(17);
        }
        self.pick(3, 11, 12, 18, 19);
        self.pick(1, 11, 12, 20, 21);
        self.pick(2, 13, 15, 22, 23);
        self.pick(7, 13, 15, 24, 25);
        self.pick(0, 13, 15, 26, 27);
        self.pick(13, 16, 17, 28, 29);
        self.pick(12, 16, 17, 30, 31);
        if self.s.p.zone == 0 && self.s.q.step == 18 {
            self.say(32);
            self.s.q.step = 19;
        }
        if self.s.p.zone == 19 && self.s.q.step == 19 {
            self.s.q.step = 20;
            self.say(33);
            self.s.q.model2 = 3;
            self.inv_remove(12, true);
            self.inv_remove(13, false);
        }
        if self.s.q.step == 20 && self.s.p.zone == 0 {
            self.say(34);
            self.s.q.step = 21;
            self.s.boss_state = -1;
        }
        if self.finale(21, 19, 35, 36, 37, 38) {
            self.events.push(Event::TextOnPicture(39, "DEPART.RON"));
            self.s.next_island = 5;
        }
        self.jingle();
    }

    /// Island 5: commando outfit, bombs, vessels (`0x160d0`).
    fn script_5(&mut self) {
        self.teleporters(&[
            (10, (-13537, 937)),
            (12, (-14025, -14775)),
            (14, (7800, -150)),
            (16, (10725, -13200)),
            (18, (13275, -7050)),
            (20, (-20400, -28500)),
        ]);
        let v = self.s.q.flags.vessel;
        // Mesh 0 = intact vessel, 1 = wreck.
        self.mesh(0, v[0], -26550.0, 0.0, 8540.0, 0.0);
        self.mesh(1, v[1], 18159.0, 0.0, -6759.0, 0.0);
        self.mesh(2, v[2], 19640.0, 0.0, 16340.0, 0.0);
        self.mesh(3, v[3], -2428.0, 0.0, -8397.0, 0.0);
        if self.input.key1 && self.s.q.step > -1 {
            self.picture("COMMANDO.RON");
        }
        if self.s.q.busy >= 1 {
            return;
        }
        if self.s.q.touch == 9 {
            self.picture("COMMANDO.RON");
            self.take_as(9);
            self.s.q.step += 1;
        }
        self.pick(0, 0, 2, 0, 1);
        self.pick(1, 0, 2, 2, 3);
        self.pick(2, 0, 2, 4, 5);
        if self.s.q.step == 3 {
            self.inv_remove(1, false);
            self.inv_remove(2, false);
            self.inv_remove(0, false);
            self.s.q.step = 4;
            self.say(6);
        }
        if self.s.q.outfit_on == 0 && self.s.q.step > 2 {
            self.events.push(Event::LoadDuckBank("DUCK_1.SPT"));
            self.s.q.outfit_on = 1;
        }
        for (id, ok, have) in [(3, 9, 8), (5, 12, 8), (4, 14, 13)] {
            if self.s.q.touch == id {
                if self.s.q.step < 3 {
                    self.refuse(7, id);
                } else if self.s.inv_has(id) {
                    self.refuse(have, id);
                } else {
                    self.take_as(id);
                    self.say(ok);
                }
            }
        }
        if self.s.q.touch == 6 {
            if self.s.q.step == 4 {
                self.take_as(6);
                self.s.q.step = 5;
                self.say(15);
            } else {
                self.refuse(16, 6);
            }
        }
        // The bomb is assembled on touching *any* quest item while holding all parts.
        if self.s.q.touch != -1
            && self.s.q.step > 4
            && self.s.inv_has(4)
            && self.s.inv_has(5)
            && self.s.inv_has(3)
        {
            self.inv_remove(4, false);
            self.inv_remove(5, false);
            self.inv_remove(3, false);
            self.inv_push(8);
            self.picture("BOMBE.RON");
        }
        if self.s.q.touch == 7 {
            if self.s.q.step < 3 {
                self.refuse(18, 7);
            } else {
                self.take_as(7);
                self.s.q.step += 1;
                self.say(17);
            }
        }
        for (k, zone) in [21, 22, 23, 24].into_iter().enumerate() {
            if self.s.p.zone == zone && self.s.q.flags.vessel[k] == 0 && self.s.inv_has(8) {
                self.s.q.flags.vessel[k] = 1;
                self.inv_remove(8, false);
                self.events.push(Event::PictureTimed("EXPLOSE.RON", 16));
                self.events.push(Event::PictureTimed("EXPLOS01.RON", 16));
                self.picture("EXPLOS02.RON");
                self.s.q.step += 1;
            }
        }
        if self.s.q.step == 9 && self.s.q.flags.vessel.iter().all(|&f| f != 0) {
            self.s.q.step = 10;
        }
        if self.s.q.step == 10 {
            self.s.boss_state = -1;
            self.s.q.step = 11;
        }
        let diff = self.s.difficulty;
        if self.s.q.step == 11 && self.s.boss_state == 2 {
            self.say(21 + diff.clamp(0, 2));
            self.s.q.step = 12;
        }
        if self.s.q.step == 12 {
            if diff == 0 {
                self.s.next_island = 6;
                self.s.q.step = 13;
            }
            if diff == 1 && self.s.alive_count < self.s.nmonsters / 2 {
                self.s.q.step = 13;
                self.say(21);
                self.s.next_island = 6;
            }
            if diff == 2 && self.s.alive_count < 2 {
                self.s.q.step = 13;
                self.say(21);
                self.s.next_island = 6;
            }
        }
        self.jingle();
    }
}
