//! The duck: `player_move` (`0x10860`), `player_shots` (`0x103b0`), `shot_explosions`
//! (`0x10290`), `player_draw` (`0x11400`), `view_keys` (`0x11960`).
//! Spec: `docs/re/game/player.md`.

use super::state::SHOTS;
use super::tables::COOLDOWN;
use super::{chop, Bank, Event, Sim, SimInput};

/// Walking speed (`0x40018`) and flying speed (`0x4001c`), units per step.
pub const SPEED: i32 = 34;
pub const FLY_SPEED: i32 = 60;
/// Turn per step is `TURN / ticks` (f64 at `0x42288`).
pub const TURN: f64 = 0.07;
/// Strafe heading offset (`0x422a0`; deliberately not pi/2).
pub const STRAFE_TURN: f64 = 1.57075;

fn blocked(zone: i32, flying: bool) -> bool {
    (zone == 14 && !flying)
        || matches!(zone, 8 | 20 | 18 | 21 | 22 | 23 | 24 | 10)
        || (zone == 13 && !flying)
}

impl Sim {
    /// One step of `player_move`.
    pub(crate) fn player_move(&mut self, inp: &SimInput) {
        let t = &self.terrain;
        let modern = self.modern;
        let p = &mut self.s.p;
        let mut sfx_drown = false;

        let prev_x = p.px;
        let prev_z = p.pz;
        p.moved = 0;
        p.move_dir = 0;
        p.strafe = 0;
        let mut ltx = p.tx;
        let mut ltz = p.tz;
        let flying = p.flying != 0;

        p.zone = t.zone_at(p.px.wrapping_neg(), p.pz.wrapping_neg());
        if !flying && p.zone == 5 && p.swim_timer > 0 {
            p.zone = 0;
            p.swim_timer -= 1;
        }
        if p.zone == 5 && !flying {
            p.drown += 1;
        }
        if p.zone == 6 {
            p.energy -= 5;
            p.hurt = 20;
        }

        let wx = p.rx.wrapping_neg();
        let wz = p.rz.wrapping_neg();
        let a = p.angle as f64;
        let (s, c) = (a.sin(), a.cos());
        // One cell ahead and under the duck (a third sample one cell behind is
        // computed by the original and never used).
        let h_front = t.height_at(chop(c * 300.0 + wx as f64), chop(s * 300.0 + wz as f64)) / 300;
        let h_here = t.height_at(wx, wz) / 300;

        let mut v;
        if !flying {
            let slope = SPEED * (h_front - h_here) / 60;
            v = if inp.down { SPEED + slope } else { SPEED - slope };
            if p.slide > 14 && v < 0 && !inp.up && !inp.down {
                // Steep slope on the icy islands: slide back.
                ltx = chop(ltx as f64 - c * v as f64);
                ltz = chop(ltz as f64 - v as f64 * s);
            }
            if p.zone == 9 {
                v /= 2;
            }
            if p.zone == 1 {
                v /= 5;
            }
            if p.zone == 2 {
                v /= 3;
            }
        } else {
            v = FLY_SPEED;
        }

        if p.drown > 10 {
            p.drown = 0;
            p.hurt = 20;
            if p.dead_wait == 0 && p.dead == 0 {
                sfx_drown = true;
            }
            p.energy -= 10;
        }

        let mut accepted = false;
        if (14..=20).contains(&p.hurt) {
            // Knock-back phase of a hit.
            let k = (v / 4) as f64;
            let (px, pz) = (p.px as f64, p.pz as f64);
            if p.last_dir == 1 {
                p.px = chop(c * k + px);
                p.pz = chop(s * k + pz);
            } else {
                p.px = chop(px - c * k);
                p.pz = chop(pz - k * s);
            }
        } else {
            if !inp.strafe {
                let d = TURN / p.ticks as f64;
                if inp.left {
                    p.angle = (p.angle as f64 - d) as f32;
                }
                if inp.right {
                    p.angle = (d + p.angle as f64) as f32;
                }
            }
            if modern.turn != 0.0 {
                p.angle = (p.angle as f64 + modern.turn as f64 / (p.ticks + 1) as f64) as f32;
            }
            // Modifier-free strafing of the modern controls; all false otherwise.
            let strafe_l = (inp.strafe && inp.left) || modern.strafe_left;
            let strafe_r = (inp.strafe && inp.right) || modern.strafe_right;
            let strafing = inp.strafe || modern.strafe_left || modern.strafe_right;
            if p.hurt == 0 {
                let back = if inp.strafe { 0.5 } else { 0.4 };
                let a = p.angle as f64;
                let (s, c) = (a.sin(), a.cos());
                let vf = v as f64;
                if inp.down {
                    ltx = chop(c * vf * back + ltx as f64);
                    ltz = chop(vf * s * back + ltz as f64);
                    p.move_dir = -1;
                }
                if inp.up {
                    ltx = chop(ltx as f64 - c * vf);
                    ltz = chop(ltz as f64 - vf * s);
                    p.move_dir = 1;
                }
                if strafing && p.zone != 1 {
                    p.angle = (p.angle as f64 + STRAFE_TURN) as f32;
                    let a2 = p.angle as f64;
                    let (s2, c2) = (a2.sin(), a2.cos());
                    if strafe_l {
                        ltx = chop(vf * 0.5 * c2 + ltx as f64);
                        ltz = chop(s2 * (vf * 0.8) + ltz as f64);
                        p.strafe = 1;
                    }
                    if strafe_r {
                        ltx = chop(ltx as f64 - vf * 0.5 * c2);
                        ltz = chop(ltz as f64 - s2 * (vf * 0.8));
                        p.strafe = 2;
                    }
                    p.angle = (p.angle as f64 - STRAFE_TURN) as f32;
                }
            }
            // From here on `zone` is the zone under the *target*.
            p.zone = t.zone_at(ltx.wrapping_neg(), ltz.wrapping_neg());

            let a = p.angle as f64;
            let (s, c) = (a.sin(), a.cos());
            let (px, pz) = (p.px as f64, p.pz as f64);
            if p.bump == 3 {
                // Walked into solid decor: pushed back a full step.
                let vf = v as f64;
                if p.last_dir == 1 {
                    p.px = chop(c * vf + px);
                    p.pz = chop(s * vf + pz);
                } else {
                    p.px = chop(px - c * vf);
                    p.pz = chop(pz - vf * s);
                }
            } else if p.bump == 2 || (p.bump == 1 && !flying) {
                let k = (if p.bump == 2 { v / 16 } else { v / 8 }) as f64;
                if inp.up {
                    p.px = chop(px - c * k);
                    p.pz = chop(pz - k * s);
                }
                if inp.down {
                    p.px = chop(c * k + p.px as f64);
                    p.pz = chop(k * s + p.pz as f64);
                }
            } else if !blocked(p.zone, flying) {
                accepted = true;
                p.tx = ltx;
                p.tz = ltz;
                if !flying {
                    if p.move_dir == 1 {
                        p.walk = p.walk.wrapping_add(1);
                        p.last_dir = 1;
                    }
                    if p.move_dir == -1 {
                        p.walk = p.walk.wrapping_sub(1);
                        p.last_dir = -1;
                    }
                } else {
                    p.walk = p.walk.wrapping_add(1);
                    if p.move_dir != 0 {
                        p.last_dir = p.move_dir;
                    }
                }
            }
        }
        if !accepted {
            p.tx = p.px;
            p.tz = p.pz;
        }

        // rx / rz still hold the position at the end of the previous step.
        p.py = t.height_at(p.rx.wrapping_neg(), p.rz.wrapping_neg());
        if p.flying == 1 {
            p.py += 15000;
        }
        if p.zone == 17 {
            p.py += 15000;
        }

        let slide = p.slide.max(1);
        if p.zone == 12 && !flying {
            p.px += (p.tx - p.px) / (slide * 4);
            p.pz += (p.tz - p.pz) / (slide * 6);
        } else {
            p.px += (p.tx - p.px) / slide;
            p.pz += (p.tz - p.pz) / slide;
        }
        if p.px != prev_x || p.pz != prev_z {
            p.moved = 1;
        }

        // World wrap, well outside the 76800-unit map.
        if p.px > 42000 {
            p.px -= 84000;
            p.tx = p.px;
        }
        if p.px < -42000 {
            p.px += 84000;
            p.tx = p.px;
        }
        if p.pz > 42000 {
            p.pz -= 84000;
            p.tz = p.pz;
        }
        if p.pz < -42000 {
            p.pz += 84000;
            p.tz = p.pz;
        }

        p.rx = p.px;
        p.rz = p.pz;
        p.cam_x = p.px;
        p.cam_y = super::terrain::CELL * 20;
        p.cam_z = p.pz;
        if p.zone < 2 {
            self.s.q.boss_warned = 0;
        }
        if sfx_drown {
            self.sfx(2, 255);
        }
    }

    /// One step of `player_shots`: cooldown, firing, shot flight.
    pub(crate) fn player_shots(&mut self, inp: &SimInput) {
        self.cluster_edi = None;
        let s = &mut self.s;
        let p = &mut s.p;
        p.fire_ctr = p.fire_ctr.wrapping_add(1);
        if p.last_weapon != p.weapon {
            p.last_weapon = p.weapon;
            p.fire_ctr = 0;
        }
        let w = p.weapon.clamp(0, 9) as usize;
        let pw = p.power.clamp(0, 2) as usize;
        if inp.fire && COOLDOWN[w][pw] < p.fire_ctr && p.ammo[w] > 0 {
            let sample = match w {
                0 | 1 | 7 => Some(13),
                2 | 6 => Some(24),
                3 | 4 => Some(6),
                5 => Some(15),
                _ => None,
            };
            if let Some(n) = sample {
                self.events.push(Event::Sfx { sample: n, volume: 255 });
            }
            if p.power_shots > 0 {
                p.power_shots -= 1;
                if p.power_shots == 0 {
                    p.power = 0;
                }
            }
            let i = s.shots.next.rem_euclid(SHOTS as i32) as usize;
            s.shots.x[i] = p.rx.wrapping_neg();
            s.shots.ty[i] = p.weapon;
            p.ammo[w] -= 1;
            s.shots.y[i] = p.py + 9000;
            s.shots.z[i] = p.rz.wrapping_neg();
            let a = (if s.autoaim_found == 0 { p.angle } else { s.autoaim_angle }) as f64;
            // Two separate rand() calls, X first.
            let r = s.rng.next() % 20 + 60;
            s.shots.vx[i] = chop(r as f64 * a.cos());
            let r = s.rng.next() % 20 + 60;
            s.shots.vz[i] = chop(r as f64 * a.sin());
            s.shots.state[i] = if p.weapon == 7 { 49 } else { 99 };
            s.shots.next = (s.shots.next + 1) % SHOTS as i32;
            p.fire_ctr = 0;
            p.shooting = 2;
        }

        // Flight. (The original loop runs one slot past the pool; that slot aliases
        // other arrays and is never in the flying range in practice, so it is not
        // modelled.)
        for i in 0..SHOTS {
            let st = self.s.shots.state[i];
            if !(st > 0 && st < 100) {
                continue;
            }
            let sh = &mut self.s.shots;
            let ground = self.terrain.height_at(sh.x[i], sh.z[i]);
            sh.state[i] -= 1;
            sh.x[i] = sh.x[i].wrapping_add(sh.vx[i]);
            sh.z[i] = sh.z[i].wrapping_add(sh.vz[i]);
            let (ty, x, y, z, st) = (sh.ty[i], sh.x[i], sh.y[i], sh.z[i], sh.state[i]);
            if ty != 5 {
                self.sprite(st % 2 + 0x23 + ty * 2, Bank::Boom, 0, x, y, z, 4.0);
            }
            if y < ground || (ty == 7 && st == 20) {
                self.s.shots.state[i] = 200;
                if ty == 7 {
                    self.cluster(i);
                }
            }
        }
    }

    /// Weapon 7: 39 delayed explosions scattered around slot `i` (`0x1078a`).
    fn cluster(&mut self, i: usize) {
        let fix_last = self.enh.fix_cluster_last_slot_type;
        let s = &mut self.s;
        for k in 1..=39usize {
            // As compiled, the type is written through the slot index of the
            // *previous* iteration (register EDI), so slots i+1..i+38 become type 7
            // and slot i+39 keeps its old type. On the very first write of a step the
            // register is stale (a wild write in the original); not modelled.
            if let Some(prev) = self.cluster_edi {
                s.shots.ty[prev] = s.shots.ty[i];
            }
            let j = (i + k) % SHOTS;
            self.cluster_edi = Some(j);
            if fix_last {
                s.shots.ty[j] = s.shots.ty[i];
            }
            s.shots.state[j] = s.rng.next() % 10 + 200;
            s.shots.x[j] = s.rng.next() % 2400 - 1200 + s.shots.x[i];
            s.shots.z[j] = s.rng.next() % 2400 - 1200 + s.shots.z[i];
        }
    }

    /// Per frame: explosion animation of player shots.
    pub(crate) fn shot_explosions(&mut self) {
        let fix_spent = self.enh.fix_spent_shot_slots;
        for i in 0..SHOTS {
            let st = self.s.shots.state[i];
            if st <= 100 {
                continue;
            }
            if st < 201 {
                let (ty, x, y, z) =
                    (self.s.shots.ty[i], self.s.shots.x[i], self.s.shots.y[i], self.s.shots.z[i]);
                if st == 200 {
                    if ty < 2 {
                        self.sfx(16, 255);
                    } else if ty != 5 {
                        self.sfx(1, 255);
                    }
                }
                if ty == 7 {
                    self.sprite(230 - st, Bank::Boom, 0, x, -300, z, 6.0);
                } else {
                    self.sprite(200 - st + ty * 5, Bank::Boom, 0, x, y, z, 6.0);
                }
                if st < 197 {
                    self.s.shots.state[i] = 0;
                }
            }
            // Original quirk: after the last explosion frame the slot ends at -1, not
            // 0, and keeps hitting monsters (entities.md 3.3).
            self.s.shots.state[i] -= 1;
            if fix_spent && self.s.shots.state[i] == -1 {
                self.s.shots.state[i] = 0;
            }
        }
    }

    /// Per frame: duck sprite selection, death sequence and respawn.
    pub(crate) fn player_draw(&mut self) {
        let p = &self.s.p;
        let a = p.angle as f64;
        let wx = p.rx.wrapping_neg();
        let wz = p.rz.wrapping_neg();
        let py = p.py;
        // Marker 10 cells ahead at height 0 (purpose unknown, open-questions Q6).
        let mx = chop(a.cos() * 3000.0 + wx as f64);
        let mz = chop(a.sin() * 3000.0 + wz as f64);
        self.sprite(0x2d, Bank::Duck, 0, mx, 0, mz, -999.0);

        let p = &mut self.s.p;
        let mut frame: Option<i32> = None;
        if p.dead != 0 {
            p.energy = 0;
            frame = Some((25 - p.dead) / 5 + if p.zone == 1 { 0x33 } else { 0x28 });
            if p.dead > 2 && p.dead_wait == 0 {
                p.dead -= 1;
            }
            if p.dead_wait > 0 {
                p.dead_wait -= 1;
            }
            if p.dead == 2 {
                p.dead_wait = 50;
                p.dead = 1;
            }
            if p.dead_wait == 10 {
                p.dead_wait = 0;
                p.dead = 0;
                p.energy = 99;
                p.hurt = 0;
                p.px = p.start_x;
                p.pz = p.start_z;
                p.tx = p.start_x;
                p.tz = p.start_z;
                p.lives -= 1;
                self.events.push(Event::FadeIn);
                self.events.push(Event::CdTrack(5));
            }
        } else if (14..=20).contains(&p.hurt) {
            frame = Some((20 - p.hurt) / 2 + if p.flying == 0 { 0x18 } else { 0x1c });
        } else if p.hurt % 2 != 0 {
            // Blinking: not drawn on odd values.
        } else if p.flying == 0 && p.strafe != 0 {
            frame = Some((p.strafe_ctr / 6) % 4 + if p.strafe == 1 { 0x20 } else { 0x24 });
            p.strafe_ctr = p.strafe_ctr.wrapping_add(1);
        } else if p.flying == 0 && p.zone == 1 {
            let idx = (p.walk / 6) % 6;
            if idx == 1 && p.wade_latch == 0 {
                p.wade_latch = 1;
                self.events.push(Event::Sfx { sample: 8, volume: 255 });
            } else if idx != 1 && p.wade_latch == 1 {
                p.wade_latch = 0;
            }
            frame = Some(idx + 0x2d);
        } else if p.moved == 0 && p.flying == 0 {
            frame = Some(if p.shooting == 0 { 0x38 } else { 0x39 });
        } else {
            let base = (1 - p.flying) * if p.shooting == 0 { 0x0c } else { 0x12 };
            frame = Some(base + (p.walk / 6) % 6);
        }
        if p.shooting > 0 {
            p.shooting -= 1;
        }
        p.bump = 0;
        if let Some(f) = frame {
            self.sprite(f, Bank::Duck, 0, wx, py, wz, -1.0);
        }
    }

    /// Per frame: F1 / F2, weapon select, jetpack toggle.
    pub(crate) fn view_keys(&mut self) {
        let inp = self.input;
        if inp.view {
            self.sfx(12, 255);
            self.events.push(Event::ViewCycle);
        }
        if inp.interlace {
            self.events.push(Event::InterlaceToggle);
        }
        if inp.next_weapon {
            self.sfx(23, 255);
            let p = &mut self.s.p;
            p.weapon = (p.weapon + 1) % 10;
            while p.weapon < 10
                && (p.owned[p.weapon as usize] == 0 || p.ammo[p.weapon as usize] < 1)
            {
                p.weapon += 1;
            }
            if p.weapon > 9 {
                p.weapon = 0;
            }
        }
        if let Some(w) = self.modern.select_weapon.take() {
            let p = &mut self.s.p;
            let w = w.min(9) as usize;
            if p.weapon as usize != w && p.owned[w] != 0 && p.ammo[w] >= 1 {
                p.weapon = w as _;
                self.sfx(23, 255);
            }
        }
        if inp.jetpack && self.s.p.fuel > 0 {
            self.s.p.flying = (self.s.p.flying + 1) % 2;
            if self.s.p.flying == 1 {
                self.sfx(5, 255);
            }
        }
    }
}
