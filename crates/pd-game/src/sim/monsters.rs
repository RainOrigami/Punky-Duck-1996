//! Monsters and the boss: spawn (`0x24110`), per-frame update (`0x21af0`), idle path
//! following (`0x22c60` kind 5), animation (`0x23860`), facing and auto-aim (`0x236a0`),
//! hits (`0x23be0`), monster shots (`0x23260`), body contact (`0x23ec0`), terrain
//! constraint (`0x218b0`) and the sprite queue calls of `0x11be0`.
//! Spec: `docs/re/game/entities.md`.

use super::state::{Monster, MONSTERS, SHOTS, TRJ_BUFFER_ADDR};
use super::tables::BOSS_MUZZLE;
use super::{chop, Bank, Event, Sim};

/// The game's `atan2f(a, b)` helper (`0x15230`): `atan(b / a)`, plus pi when `a < 0`
/// and `b >= 0`, minus pi when both are negative; special cases for `|a| < 1e-10`.
pub fn atan2f(a: f32, b: f32) -> f32 {
    if (a.abs() as f64) >= 1e-10 {
        let mut r = (b as f64 / a as f64).atan() as f32;
        if b >= 0.0 && a < 0.0 {
            r = (r as f64 + 3.1415926535) as f32;
        }
        if b < 0.0 && a < 0.0 {
            r = (r as f64 - 3.1415926535) as f32;
        }
        r
    } else if b > 0.0 {
        1.5707964
    } else if b >= 0.0 {
        3.1415927
    } else {
        -1.5707964
    }
}

/// Facing octant: `trunc((heading - (angle - 0.3927)) / 0.7854) & 7` (`0x2377b`, `0x2357a`).
fn octant(heading: f32, angle: f32) -> i32 {
    let t = heading as f64 - (angle as f64 + -0.3926990875);
    chop(t / 0.785398175) & 7
}

impl Sim {
    /// Run-time values of the `path` / `rec` pointers of monster slot `i`.
    pub(crate) fn monster_ptrs(&self, i: usize) -> (u32, u32) {
        let npaths = self.monster_trj.paths.len() as u32;
        let rec = TRJ_BUFFER_ADDR + 4 + npaths * 8 + 4 + i as u32 * 32;
        let path = self.monster_trj.records.get(i).map(|r| r.path).unwrap_or(0) as u32;
        (TRJ_BUFFER_ADDR + 4 + path.wrapping_mul(8), rec)
    }

    /// `{first_point, point_count}` of the path of record `i`.
    fn path_of(&self, i: usize) -> (usize, i32) {
        let trj = &self.monster_trj;
        trj.records
            .get(i)
            .and_then(|r| usize::try_from(r.path).ok())
            .and_then(|p| trj.paths.get(p))
            .map(|p| (p.first_point as usize, p.point_count as i32))
            .unwrap_or((0, 1))
    }

    fn path_point(&self, idx: usize) -> (f64, f64) {
        self.monster_trj.points.get(idx).map(|p| (p[0] as f64, p[1] as f64)).unwrap_or((0.0, 0.0))
    }

    /// `W(c, P)`: path point `P` rotated by the record angle and scaled around `c`
    /// (`0x230a7`).
    fn waypoint(&self, i: usize, c: (i32, i32), p: (f64, f64)) -> (i32, i32) {
        let m = &self.s.monsters[i];
        let scale = self.monster_trj.records.get(i).map(|r| r.scale).unwrap_or(1.0) as f64;
        let (cs, sn) = (m.rot_cos as f64, m.rot_sin as f64);
        let (cx, cz) = (c.0 as f64, c.1 as f64);
        let x = chop(((p.1 - cz) * sn + (p.0 - cx) * cs) * scale + cx);
        let z = chop(((p.1 - cz) * cs + -sn * (p.0 - cx)) * scale + cz);
        (x, z)
    }

    fn set_segment_step(m: &mut Monster) {
        if m.seg_len == 0 {
            m.step_x = 0;
            m.step_z = 0;
        } else {
            m.step_x = m.speed_x.wrapping_mul(m.seg_x1 - m.seg_x0) / m.seg_len;
            m.step_z = m.speed_z.wrapping_mul(m.seg_z1 - m.seg_z0) / m.seg_len;
        }
    }

    fn seg_length(m: &Monster) -> i32 {
        let dx = m.seg_x1.wrapping_sub(m.seg_x0);
        let dz = m.seg_z1.wrapping_sub(m.seg_z0);
        chop((dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) as f64).sqrt())
    }

    /// Create monster `i` from TRJ record `i` (body of `monsters_init`, also used by
    /// the random respawn).
    pub(crate) fn spawn_monster(&mut self, i: usize) {
        let Some(rec) = self.monster_trj.records.get(i).copied() else {
            return;
        };
        let diff = self.s.difficulty;
        let island = self.s.island;
        let mut m = Monster { ty: rec.kind + 1, move_kind: 5, ai: 0, y: 300, ..Default::default() };
        m.speed_x = 100;
        m.speed_z = 100;
        let sq = |cells: i32| (cells * 300 * cells * 300) / 1000;
        m.detect = sq(diff * 5 + 12);
        let cls = self.s.class_of(m.ty);
        m.attack = match cls {
            2 => 90,
            3 => 360,
            _ => sq(diff * 3 + 6),
        };
        m.leash = sq(diff * 10 + 20);
        m.hp = (m.ty + island * 2) * (diff + 1) + 2;
        m.anim_period = 1;
        if cls != 2 && cls != 3 {
            m.shot_speed = diff * 50 + 150;
            m.max_shots = diff + 1;
            m.spread = 600;
            m.fire_chance = diff * 20 + 30;
        }
        let (path_ptr, rec_ptr) = self.monster_ptrs(i);
        m.path_ptr = path_ptr;
        m.rec_ptr = rec_ptr;
        m.path_idx = 1;
        m.rot_cos = (rec.angle as f64).cos() as f32;
        m.rot_sin = (rec.angle as f64).sin() as f32;
        self.s.monsters[i] = m;

        let (first, count) = self.path_of(i);
        let p0 = self.path_point(first);
        let seg0 = (chop(p0.0), chop(p0.1));
        let seg1 = if count < 2 { seg0 } else { self.waypoint(i, seg0, self.path_point(first + 1)) };
        let m = &mut self.s.monsters[i];
        m.seg_x0 = seg0.0 + rec.x;
        m.seg_z0 = seg0.1 + rec.z;
        m.seg_x1 = seg1.0 + rec.x;
        m.seg_z1 = seg1.1 + rec.z;
        m.seg_len = Self::seg_length(m);
        Self::set_segment_step(m);
        m.x = m.seg_x0;
        m.z = m.seg_z0;
    }

    /// `monsters_init` (`0x24110`).
    pub(crate) fn monsters_init(&mut self) {
        for m in self.s.monsters.iter_mut() {
            *m = Monster::default();
        }
        for i in 0..(self.s.nmonsters.max(0) as usize).min(MONSTERS) {
            self.spawn_monster(i);
        }
    }

    fn make_boss(&mut self) {
        let diff = self.s.difficulty;
        let island = self.s.island;
        let m = &mut self.s.monsters[0];
        m.ty = 5;
        m.speed_x = 100;
        m.speed_z = 100;
        m.hp = (diff + 1) * island * 20 + 150;
        m.anim_period = 2;
        m.move_kind = 1;
        m.y = 300;
        m.ai = 0;
        m.x = 0;
        m.z = 0;
        m.attack = 4500;
        m.detect = 0x168000;
        m.leash = 0x168000;
        m.spread = 300;
        m.shot_speed = diff * 50 + 150;
        m.max_shots = 5;
        m.fire_chance = diff * 50 + 100;
    }

    /// `monsters_draw` (`0x11be0`): update, then queue the sprites.
    pub(crate) fn monsters_frame(&mut self) {
        let (wx, wz) = self.s.duck_world();
        let py = self.s.p.py;
        let alive = self.s.p.dead == 0;
        self.monsters_update(wx, wz, py, alive);
        for i in 0..MONSTERS {
            let m = &self.s.monsters[i];
            if m.ty <= 0 {
                continue;
            }
            let boss = i == 0 && self.s.boss_state == 1;
            let (bank, scale, shot_scale) =
                if boss { (Bank::Boss, 12.0, 8.0) } else { (Bank::Enemy, 4.0, 4.0) };
            let (frame, flags, x, y, z) = (m.frame, m.hit * 8 + m.mirror, m.x, m.y.wrapping_neg(), m.z);
            let n = m.max_shots.clamp(0, 5) as usize;
            let shots: [(i32, i32, i32, i32, i32, i32); 5] = core::array::from_fn(|k| {
                (m.shot_state[k], m.shot_frame[k], m.shot_mirror[k], m.shot_x[k], m.shot_y[k], m.shot_z[k])
            });
            self.sprite(frame, bank, flags, x, y, z, scale);
            for &(st, f, mir, sx, sy, sz) in shots.iter().take(n) {
                if st != 0 {
                    self.sprite(f, bank, mir, sx, sy, sz, shot_scale);
                }
            }
        }
    }

    /// `monsters_update(wx, wz, py, player_alive)` (`0x21af0`), once per frame.
    pub(crate) fn monsters_update(&mut self, wx: i32, wz: i32, py: i32, player_alive: bool) {
        if self.s.p.angle > 0.0 {
            self.s.p.angle = (self.s.p.angle as f64 + -6.2831854) as f32;
        }
        self.s.alive_count = 0;
        self.s.autoaim_found = 0;
        self.s.autoaim_best = f32::from_bits(0x4049_0fdb);
        let mut spawned = 0;

        if self.s.boss_state == -1 {
            self.events.push(Event::LoadBossBank);
            // `0x415a0 = 1`: the byte after the four class bytes (saved in save games).
            self.s.class_bytes[4] = 1;
            // Slot 0 is not cleared here: whatever monster 0 was doing (animation,
            // shots in flight, path fields) carries over into the boss.
            self.make_boss();
            self.s.boss_state = 1;
            if self.s.island == 5 {
                self.s.boss_respawns = 3;
            }
        }
        if self.s.boss_state == 1 {
            if self.s.island == 5 && self.s.monsters[0].ty == 0 {
                self.s.boss_respawns -= 1;
                if self.s.boss_respawns != 0 {
                    self.s.monsters[0] = Monster::default();
                    self.make_boss();
                }
            }
            if self.s.monsters[0].ty == 0 {
                self.s.boss_state = 2;
            }
        }

        // Pass 1: respawn rolls and the active list.
        let mut active: Vec<usize> = Vec::new();
        for i in 0..MONSTERS {
            if self.s.monsters[i].ty == 0 {
                if self.s.boss_state == 0
                    && spawned == 0
                    && (self.s.rng.next() & 0xfff) == 0
                    && (i as i32) < self.s.nmonsters
                {
                    self.spawn_monster(i);
                    spawned += 1;
                }
                continue;
            }
            self.s.alive_count += 1;
            let m = &mut self.s.monsters[i];
            let near = m.x.wrapping_sub(wx).wrapping_abs() < 6000
                && m.z.wrapping_sub(wz).wrapping_abs() < 6000;
            if near || (self.s.boss_state == 1 && i == 0) {
                active.push(i);
            } else {
                m.shot_state = [0; 5];
            }
        }

        // Pass 2.
        for i in active {
            let m = &mut self.s.monsters[i];
            m.prev_x = m.x;
            m.prev_z = m.z;
            m.prev_y = m.y;
            let dz = (wz.wrapping_sub(m.z)) as f64;
            let dx = (wx.wrapping_sub(m.x)) as f64;
            let d2 = ((dz * dz + dx * dx) / 1000.0) as f32;
            m.dist2 = chop((dz * dz + dx * dx) / 1000.0);
            let hz = (m.home_z.wrapping_sub(m.z)) as f64;
            let hx = (m.home_x.wrapping_sub(m.x)) as f64;
            let d2h = ((hz * hz + hx * hx) / 1000.0) as f32;
            let dist = ((d2 as f64 * 1000.0).sqrt()) as f32;
            match m.ai {
                0 => {
                    if (m.detect as f32) <= d2 {
                        self.idle_move(i);
                    } else {
                        m.home_x = m.x;
                        m.ai = 1;
                        m.home_z = m.z;
                        let detect = m.detect;
                        let r = self.s.rng.next();
                        let vol = chop(255.0 - (d2 as f64 * 200.0) as f32 as f64 / detect as f64);
                        self.sfx(if r & 1 == 0 { 4 } else { 28 }, vol);
                    }
                }
                1 => {
                    if m.move_kind != 0 {
                        if d2h < m.leash as f32 {
                            // Axis-wise pursuit, no normalisation.
                            if m.x < wx {
                                m.x += m.speed_x;
                            }
                            if wx < m.x {
                                m.x -= m.speed_x;
                            }
                            if m.z < wz {
                                m.z += m.speed_z;
                            }
                            if wz < m.z {
                                m.z -= m.speed_z;
                            }
                        }
                        if m.anim_idx == 0 {
                            if d2 < m.attack as f32 {
                                m.ai = 2;
                            } else if (m.detect as f32) <= d2 {
                                m.ai = 3;
                            }
                        }
                    }
                }
                2 => {
                    if d2 < m.attack as f32 {
                        let n = m.max_shots.clamp(0, 5) as usize;
                        let k = (0..n).find(|&k| m.shot_state[k] == 0).unwrap_or(n);
                        if player_alive && k < n {
                            let chance = m.fire_chance;
                            if (self.s.rng.next() & 0xff) < chance {
                                self.monster_fire(i, k, wx, wz, py, d2, dist);
                            }
                        }
                    } else {
                        m.ai = 1;
                    }
                }
                3 => {
                    if d2 < m.attack as f32 {
                        m.ai = 2;
                    } else if d2 < m.detect as f32 {
                        m.ai = 1;
                    } else if m.x == m.home_x && m.z == m.home_z {
                        m.ai = 0;
                    } else if m.move_kind != 0 {
                        if m.x < m.home_x {
                            m.x += m.speed_x;
                        }
                        if m.home_x < m.x {
                            m.x -= m.speed_x;
                        }
                        if m.z < m.home_z {
                            m.z += m.speed_z;
                        }
                        if m.home_z < m.z {
                            m.z -= m.speed_z;
                        }
                        if (m.x - m.home_x).abs() < m.speed_x {
                            m.x = m.home_x;
                        }
                        if (m.z - m.home_z).abs() < m.speed_z {
                            m.z = m.home_z;
                        }
                    }
                }
                _ => {}
            }
            self.monster_animate(i);
            self.monster_face(i, wx, wz);
            self.monster_hit(i);
            self.monster_shots(i, wx, wz, py, dist);
            self.monster_contact(i, wx, wz, py);
            self.monster_zone(i);
            let m = &mut self.s.monsters[i];
            if m.x < -38400 {
                m.x += 76800;
            }
            if m.z < -38400 {
                m.z += 76800;
            }
            if m.x > 38099 {
                m.x -= 76800;
            }
            if m.z > 38099 {
                m.z -= 76800;
            }
            if m.move_kind == 5 && self.path_of(i).1 == 1 {
                // Monsters on a single-point path never change position.
                let m = &mut self.s.monsters[i];
                m.x = m.prev_x;
                m.z = m.prev_z;
                m.y = m.prev_y;
            }
        }
    }

    /// Idle movement, `move_kind` 5 (`0x22fbe`). Kinds 2..4 are dead code in the
    /// original and not ported.
    fn idle_move(&mut self, i: usize) {
        let fix = self.enh.fix_monster_path_drift;
        let m = &self.s.monsters[i];
        if m.move_kind != 5 {
            return;
        }
        let arrived = if fix {
            (m.x - m.seg_x1).abs() <= m.step_x.abs() && (m.z - m.seg_z1).abs() <= m.step_z.abs()
        } else {
            // Original: absolute distance compared with the *signed* step, so a leg
            // whose step has a zero or negative component is never completed.
            (m.x.wrapping_sub(m.seg_x1)).wrapping_abs() < m.step_x
                && (m.z.wrapping_sub(m.seg_z1)).wrapping_abs() < m.step_z
        };
        if !arrived {
            let m = &mut self.s.monsters[i];
            m.x = m.x.wrapping_add(m.step_x);
            m.z = m.z.wrapping_add(m.step_z);
            return;
        }
        let (first, count) = self.path_of(i);
        let m = &mut self.s.monsters[i];
        m.x = m.seg_x1;
        m.path_idx += 1;
        m.z = m.seg_z1;
        m.seg_x0 = m.seg_x1;
        m.seg_z0 = m.seg_z1;
        if m.path_idx == count {
            m.path_idx = 0;
        }
        let idx = m.path_idx;
        if count > 1 {
            let seg0 = (m.seg_x0, m.seg_z0);
            let p = self.path_point(first + idx.max(0) as usize);
            let seg1 = if fix {
                // Evident intent: the same construction as at spawn, around the
                // path-local pivot, offset by the record position.
                let rec = self.monster_trj.records[i];
                let p0 = self.path_point(first);
                let pivot = (chop(p0.0), chop(p0.1));
                let w = self.waypoint(i, pivot, p);
                (w.0 + rec.x, w.1 + rec.z)
            } else {
                // Original: the pivot is the monster's *world* position while the
                // point is path-local, and the record position is not added.
                self.waypoint(i, seg0, p)
            };
            let m = &mut self.s.monsters[i];
            m.seg_x1 = seg1.0;
            m.seg_z1 = seg1.1;
            m.seg_len = Self::seg_length(m);
        } else {
            m.seg_x1 = m.seg_x0;
            m.seg_z1 = m.seg_z0;
        }
        let m = &mut self.s.monsters[i];
        if m.seg_len == 0 {
            return;
        }
        Self::set_segment_step(m);
    }

    /// `0x227xx`: launch shot `k` of monster `i` at the duck.
    #[allow(clippy::too_many_arguments)]
    fn monster_fire(&mut self, i: usize, k: usize, wx: i32, wz: i32, py: i32, d2: f32, dist: f32) {
        let spread = self.s.monsters[i].spread;
        let rx = self.s.rng.next() % (spread + 1) - (spread >> 1);
        let rz = self.s.rng.next() % (spread + 1) - (spread >> 1);
        let island = self.s.island;
        let m = &mut self.s.monsters[i];
        m.shot_x[k] = m.x;
        m.shot_z[k] = m.z;
        m.shot_y[k] = self.terrain.height_at(m.x, m.z) + m.y + 4000;
        if m.ty == 5 {
            m.shot_y[k] += BOSS_MUZZLE[island.clamp(0, 5) as usize];
        }
        let dx = (wx - (m.x + rx)) as f32;
        let dz = (wz - (m.z + rz)) as f32;
        let dy = ((py + 6500) - m.shot_y[k]) as f32;
        let (nx, nz, ny) = if d2.to_bits() & 0x7fff_ffff == 0 {
            (1.0f32, 1.0f32, 1.0f32)
        } else {
            (
                (dx as f64 / dist as f64) as f32,
                (dz as f64 / dist as f64) as f32,
                (dy as f64 / dist as f64) as f32,
            )
        };
        let sp = m.shot_speed as f64;
        m.shot_vx[k] = chop(sp * nx as f64);
        m.shot_vz[k] = chop(sp * nz as f64);
        m.shot_vy[k] = chop(sp * ny as f64);
        m.shot_anim[k] = 0;
        m.shot_state[k] = 1;
    }

    /// `0x23860`: choose the animation, compute the frame, advance, free dead monsters.
    fn monster_animate(&mut self, i: usize) {
        let m = &mut self.s.monsters[i];
        let t = self.s.sprite_tables[(m.ty - 1).clamp(0, 4) as usize];
        let (base, walk_off, attack_off, stand_off, death_off) = (t[0], t[1], t[2], t[3], t[4]);
        let (walk_n, attack_n, stand_n, death_n) = (t[6], t[7], t[8], t[9]);
        if m.anim != 3 {
            // A shot launched this very frame (state exactly 1) starts the attack
            // animation.
            let n = m.max_shots.clamp(0, 5) as usize;
            let firing = m.shot_state[..n].contains(&1);
            if m.ai == 4 {
                m.anim = 3;
                m.anim_idx = 0;
            } else if firing {
                m.anim = 1;
                m.anim_idx = 0;
            } else if m.anim != 1 || attack_n <= m.anim_idx {
                let want = if m.x == m.prev_x && m.z == m.prev_z { 2 } else { 0 };
                if m.anim != want {
                    m.anim = want;
                    m.anim_idx = 0;
                }
            }
        }
        let dir5 = if m.dir > 4 { 8 - m.dir } else { m.dir };
        m.mirror = (m.dir > 4) as i32;
        let mut frame = base + 8;
        match m.anim {
            0 => frame += dir5 * walk_n + walk_off,
            1 => frame += dir5 * attack_n + attack_off,
            2 => frame += dir5 * stand_n + stand_off,
            3 => frame += death_off,
            _ => {}
        }
        m.frame = frame + m.anim_idx;
        m.anim_tick += 1;
        if m.anim_tick == m.anim_period {
            m.anim_tick = 0;
            m.anim_idx += 1;
            match m.anim {
                0 if walk_n != 0 => m.anim_idx %= walk_n,
                1 => m.anim_idx %= attack_n + 1,
                2 if stand_n != 0 => m.anim_idx %= stand_n,
                3 => {
                    if m.anim_idx == death_n {
                        m.ty = 0;
                        m.shot_state = [0; 5];
                    }
                }
                _ => {}
            }
        }
    }

    /// `0x236a0`: facing octant and auto-aim candidate.
    fn monster_face(&mut self, i: usize, wx: i32, wz: i32) {
        let angle = self.s.p.angle;
        let m = &mut self.s.monsters[i];
        let mut hx = m.x.wrapping_sub(m.prev_x);
        let mut hz = m.z.wrapping_sub(m.prev_z);
        if hx == 0 && hz == 0 {
            hx = wx.wrapping_sub(m.x);
            hz = wz.wrapping_sub(m.z);
        }
        let bearing = atan2f(m.x.wrapping_sub(wx) as f32, m.z.wrapping_sub(wz) as f32);
        let heading = atan2f(hx as f32, hz as f32);
        m.dir = octant(heading, angle);
        let u = (bearing as f64 - angle as f64 + 0.19634954375).abs();
        let v = u - (u / 6.2831854).trunc() * 2.0 * 3.1415927;
        let vf = v as f32;
        if v < 0.3926990875 && vf < self.s.autoaim_best {
            self.s.autoaim_found = 1;
            self.s.autoaim_angle = bearing;
            self.s.autoaim_best = vf;
        }
    }

    /// `0x23be0`: player shots against monster `i`.
    fn monster_hit(&mut self, i: usize) {
        let by_shot = self.enh.monster_damage_uses_shot_type;
        self.s.monsters[i].hit = 0;
        for j in 0..SHOTS {
            let m = &self.s.monsters[i];
            if m.ai == 4 || self.s.shots.state[j] == 0 {
                continue;
            }
            let sh = &self.s.shots;
            if sh.x[j].wrapping_sub(m.x).wrapping_abs() >= 250
                || sh.z[j].wrapping_sub(m.z).wrapping_abs() >= 250
            {
                continue;
            }
            let h = (sh.y[j] / 16).wrapping_abs();
            let ground = self.terrain.height_at(m.x, m.z);
            if h - (ground - m.y) >= 8000 {
                continue;
            }
            // Original: damage from the weapon selected *now*, not the shot's type.
            let wpn = if by_shot { sh.ty[j] } else { self.s.p.weapon };
            let boss_alive = self.s.boss_state == 1;
            let m = &mut self.s.monsters[i];
            m.hp -= (wpn + 2) / 2;
            m.hit = 1;
            let vol = 255 - if m.detect != 0 { m.dist2.wrapping_mul(200) / m.detect } else { 0 };
            let odd = j & 1 != 0;
            if m.hp < 1 {
                m.move_kind = 0;
                m.ai = 4;
                m.anim_period = 2;
                self.sfx(if odd { 17 } else { 18 }, vol);
                if boss_alive {
                    self.sfx(1, vol);
                }
            } else {
                self.sfx(if odd { 19 } else { 20 }, vol);
            }
            self.s.shots.state[j] = 0;
        }
    }

    /// `0x23260`: flight and impact of the monster's own shots.
    fn monster_shots(&mut self, i: usize, wx: i32, wz: i32, py: i32, dist: f32) {
        let angle = self.s.p.angle;
        let dead = self.s.p.dead;
        let n = self.s.monsters[i].max_shots.clamp(0, 5) as usize;
        for k in 0..n {
            let m = &mut self.s.monsters[i];
            let st = m.shot_state[k];
            if st == 0 {
                continue;
            }
            if st < 1 {
                // Impact animation: frames 1..7 of the bank.
                m.shot_frame[k] = st + 101;
                m.shot_state[k] = st + 1;
                if st + 101 > 7 {
                    m.shot_state[k] = 0;
                }
                continue;
            }
            let sub = m.shot_speed / 30;
            let (vx, vz, vy) = (m.shot_vx[k], m.shot_vz[k], m.shot_vy[k]);
            let (ox, oz) = (m.shot_x[k], m.shot_z[k]);
            m.shot_state[k] += 1;
            for _ in 0..sub {
                let m = &mut self.s.monsters[i];
                m.shot_x[k] += vx / sub;
                m.shot_z[k] += vz / sub;
                m.shot_y[k] += vy / sub;
                if m.shot_y[k] < 0 {
                    m.shot_y[k] = 0;
                }
                let (sx, sz, sy) = (m.shot_x[k], m.shot_z[k], m.shot_y[k]);
                if sx < -38400 || sz < -38400 || sx > 38100 || sz > 38100 || m.shot_state[k] > 49 {
                    m.shot_state[k] = 0;
                    break;
                }
                if self.terrain.flags_at(sx, sz) & 8 != 0 {
                    m.shot_state[k] = 0;
                    break;
                }
                if sy < self.terrain.height_at(sx, sz) {
                    m.shot_state[k] = -100;
                    if m.ty != 5 && 600.0 < dist {
                        m.ai = 1;
                    }
                    break;
                }
                if wx - 100 < sx
                    && sx < wx + 100
                    && wz - 100 < sz
                    && sz < wz + 100
                    && py - 100 < sy
                    && sy < py + 10000
                {
                    m.shot_state[k] = -100;
                    if dead == 0 {
                        // No hurt timer: monster shots only drain energy.
                        self.s.p.energy -= m.ty;
                    }
                    m.shot_x[k] = wx;
                    m.shot_z[k] = wz;
                    m.shot_y[k] = py + 6500;
                    break;
                }
            }
            let m = &mut self.s.monsters[i];
            let t = self.s.sprite_tables[(m.ty - 1).clamp(0, 4) as usize];
            let (base, shot_off, shot_n) = (t[0], t[5], t[10]);
            let heading = atan2f((m.shot_x[k] - ox) as f32, (m.shot_z[k] - oz) as f32);
            let mut d = octant(heading, angle);
            if d < 5 {
                m.shot_mirror[k] = 0;
            } else {
                d = 8 - d;
                m.shot_mirror[k] = 1;
            }
            if shot_n != 0 {
                m.shot_anim[k] = (m.shot_anim[k] + 1) % shot_n;
            }
            m.shot_frame[k] = base + 8 + shot_off + d * shot_n + m.shot_anim[k];
        }
    }

    /// `0x23ec0`: touching a monster.
    fn monster_contact(&mut self, i: usize, wx: i32, wz: i32, py: i32) {
        let m = &self.s.monsters[i];
        if wx.wrapping_sub(m.x).wrapping_abs() >= 125 || wz.wrapping_sub(m.z).wrapping_abs() >= 125 {
            return;
        }
        let top = self.terrain.height_at(m.x, m.z) + m.y;
        if (py - top).wrapping_abs() >= 8000 {
            return;
        }
        let r = self.s.rng.next() % 50 + 1;
        if r * 10 < self.s.p.money {
            self.s.p.money -= r * 10;
        }
        self.s.p.energy -= 1;
        self.s.p.hurt = 20;
    }

    /// `0x218b0`: terrain constraint by movement class (not for the boss).
    fn monster_zone(&mut self, i: usize) {
        let m = &self.s.monsters[i];
        let flags = self.terrain.flags_at(m.x, m.z);
        if m.ty == 5 {
            return;
        }
        let cls = self.s.class_of(m.ty);
        let m = &mut self.s.monsters[i];
        let revert = |m: &mut Monster| {
            m.x = m.prev_x;
            m.z = m.prev_z;
            m.y = m.prev_y;
        };
        match cls {
            0 | 3 => {
                if flags & 2 != 0 {
                    revert(m);
                }
                if flags & 4 != 0 {
                    m.x = (m.prev_x + m.x) / 2;
                    m.z = (m.prev_z + m.z) / 2;
                    m.y = (m.prev_y + m.y) / 2;
                }
            }
            1 => {
                if flags & 1 != 0 {
                    revert(m);
                }
            }
            2 => {
                if flags & 0x10 == 0 {
                    revert(m);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atan2f_quadrants() {
        let pi = core::f32::consts::PI;
        assert!((atan2f(1.0, 1.0) - pi / 4.0).abs() < 1e-6);
        assert!((atan2f(-1.0, 1.0) - 3.0 * pi / 4.0).abs() < 1e-6);
        assert!((atan2f(-1.0, -1.0) + 3.0 * pi / 4.0).abs() < 1e-6);
        assert!((atan2f(1.0, -1.0) + pi / 4.0).abs() < 1e-6);
        assert_eq!(atan2f(0.0, 5.0), 1.5707964);
        assert_eq!(atan2f(0.0, -5.0), -1.5707964);
        assert_eq!(atan2f(0.0, 0.0), 3.1415927);
        // b == 0, a < 0: +pi
        assert!((atan2f(-1.0, 0.0) - pi).abs() < 1e-6);
    }

    #[test]
    fn octants() {
        // heading == angle: (0.3927 / 0.7854) = 0.5 -> 0
        assert_eq!(octant(0.0, 0.0), 0);
        assert_eq!(octant(0.8, 0.0), 1);
        assert_eq!(octant(3.2, 0.0), 4);
        // negative values wrap through the & 7
        assert_eq!(octant(-0.8, 0.0), 0);
        assert_eq!(octant(-1.3, 0.0), 7);
    }
}
