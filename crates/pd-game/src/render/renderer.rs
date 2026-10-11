//! Frame assembly: `render_frame` (`0x148a0`) and everything it calls.
//!
//! x87 code is reproduced by evaluating in `f64` and rounding to `f32`
//! wherever the original stores to a 32-bit float. `chp` is Watcom's
//! `__CHP` (`0x305a4`, truncation), `fistp` the default round-to-nearest-even.

use super::raster::{PITCH, Target, Texture, tri_fog, tri_textured};
use super::{
    Anchor, BORDER_COLOUR, CELL, HAZE_COLOUR, IslandAssets, Reach, SPRITE_FLASH, SPRITE_MIRROR, Scene,
    ViewMode,
};
use crate::settings::Enhancements;
use pd_engine::{Framebuffer, SCREEN_W};

/// Side of the ground vertex window (`0x126ec`).
const GRID: usize = 30;
/// Half of it, also the "15" of the camera code (`0x126e8`).
const HALF: i32 = 15;
const SCRATCH: usize = 0x10000;

/// What a frame reports back to the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    /// New value of `0x4f0e64` for [`super::ViewState::horizon`]: the screen
    /// row of the last drawn horizon marker sprite, or the scene's value when
    /// none was reached.
    pub horizon: i32,
    /// Pixels whose interpolated fog level left `0..=31` this frame (the
    /// original reads past the shade tables there). Diagnostic.
    pub fog_overruns: u32,
}

/// Map cells `(a, b)` (indices `a * 256 + b` into the UNI maps) covered by the
/// ground window for a camera position. `ground_build_vertices` marks these
/// in the "seen" map `0x2456a8` every frame.
pub fn ground_window(cam_x: i32, cam_z: i32) -> impl Iterator<Item = (usize, usize)> {
    let a0 = 0x80 - cam_x / CELL - HALF;
    let b0 = 0x80 - cam_z / CELL - HALF;
    (0..GRID as i32).flat_map(move |i| {
        (0..GRID as i32)
            .map(move |j| ((a0 + j).clamp(0, 255) as usize, (b0 + i).clamp(0, 255) as usize))
    })
}

#[inline]
fn to_int(x: f64) -> i32 {
    // Out of range and NaN give the x87 "integer indefinite".
    if x.is_finite() && (-2147483648.5..2147483647.5).contains(&x) { x as i32 } else { i32::MIN }
}

/// `call __CHP; fistp`: truncate toward zero.
#[inline]
fn chp(x: f64) -> i32 {
    to_int(x.trunc())
}

/// Plain `fistp`: round to nearest, ties to even.
#[inline]
fn fistp(x: f64) -> i32 {
    to_int(x.round_ties_even())
}

/// `quicksort` (`0x158c0`): sorts `keys[lo..=hi]` ascending and permutes `idx`
/// alongside. Recursion and partition scheme are the original's, so equal keys
/// end up in the same order.
fn quicksort(keys: &mut [i16], idx: &mut [i16], mut lo: i32, hi: i32) {
    loop {
        let pivot = keys[((hi as i16 as i32 + lo as i16 as i32) >> 1) as usize];
        let (mut i, mut j) = (lo, hi);
        loop {
            while pivot < keys[j as i16 as usize] {
                j -= 1;
            }
            while keys[i as i16 as usize] < pivot {
                i += 1;
            }
            if i as i16 <= j as i16 {
                keys.swap(i as i16 as usize, j as i16 as usize);
                idx.swap(i as i16 as usize, j as i16 as usize);
                i += 1;
                j -= 1;
            }
            if i as i16 > j as i16 {
                break;
            }
        }
        let j = j as i16 as i32;
        if lo < j {
            quicksort(keys, idx, lo, j);
        }
        lo = i as i16 as i32;
        if lo >= hi {
            break;
        }
    }
}

/// Camera of `camera_update` + `camera_build_matrix`.
struct Cam {
    cell_x: i32,
    cell_z: i32,
    frac_x: i32,
    frac_z: i32,
    grid_ox: i32,
    grid_oz: i32,
    base_x: f32,
    base_z: f32,
    eye: [f32; 3],
    f: [f32; 3],
    o: [f32; 3],
    r: [f32; 3],
    u: [f32; 3],
    cx: f32,
    cy: f32,
}

/// `0x15230`: the original's atan2 replacement (`yaw = atan2(b, a)`).
fn atan_like(a: f32, b: f32) -> f32 {
    if (a as f64).abs() >= 1e-10 {
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
    } else if b < 0.0 {
        -1.5707964
    } else {
        3.1415927
    }
}

impl Cam {
    fn new(scene: &Scene, lo_res: bool) -> Self {
        let c = scene.camera;
        let (cell_x, frac_x) = (c.x / CELL, c.x % CELL);
        let (cell_z, frac_z) = (c.z / CELL, c.z % CELL);
        let cs = (c.heading as f64).cos();
        let sn = (c.heading as f64).sin();
        let grid_ox = chp(cs * 1410.0 + cell_x as f64);
        let grid_oz = chp(sn * 1410.0 + cell_z as f64);
        let tx = chp(cell_x as f64 + cs * 1375.0);
        let tz = chp(cell_z as f64 + sn * 1375.0);
        // Globals are named by axis here: [x, y, z].
        let eye = [cell_x as f32, scene.view.eye_h as f32, cell_z as f32];
        let target = [tx as f32, scene.view.look_h as f32, tz as f32];

        let (sx, sy, cx, cy) =
            if lo_res { (60.0f32, 216.0f32, 160.0f32, 100.0f32) } else { (120.0, 432.0, 320.0, 240.0) };
        let d = 5.0f32;
        let mut f = [0f32; 3];
        for k in 0..3 {
            f[k] = (target[k] as f64 - eye[k] as f64) as f32;
        }
        let len = ((f[0] as f64) * (f[0] as f64)
            + (f[1] as f64) * (f[1] as f64)
            + (f[2] as f64) * (f[2] as f64))
            .sqrt();
        for v in &mut f {
            *v = (*v as f64 / len) as f32;
        }
        let mut o = [0f32; 3];
        for k in 0..3 {
            o[k] = (d as f64 * f[k] as f64 + eye[k] as f64) as f32;
        }
        let yaw = atan_like(f[2], f[0]);
        let a = yaw as f64 + 1.57079632675;
        let mut rx = a.sin() as f32;
        if a.sin().abs() < 1e-10 {
            rx = 0.0;
        }
        let ry = 0f32;
        let mut rz = a.cos() as f32;
        if a.cos().abs() < 1e-10 {
            rz = 0.0;
        }
        let mul = |a: f32, b: f32| a as f64 * b as f64;
        let mut u = [
            (mul(f[1], rz) - mul(f[2], ry)) as f32,
            (mul(f[2], rx) - mul(f[0], rz)) as f32,
            (mul(f[0], ry) - mul(f[1], rx)) as f32,
        ];
        for v in &mut u {
            *v = mul(sy, *v) as f32;
        }
        let r = [mul(sx, rx) as f32, mul(sx, ry) as f32, mul(sx, rz) as f32];
        for k in 0..3 {
            o[k] = (eye[k] as f64 - o[k] as f64) as f32;
        }
        Cam {
            cell_x,
            cell_z,
            frac_x,
            frac_z,
            grid_ox,
            grid_oz,
            base_x: (grid_ox.wrapping_add(frac_x) - HALF) as f32,
            base_z: (grid_oz.wrapping_add(frac_z) - HALF) as f32,
            eye,
            f,
            o,
            r,
            u,
            cx,
            cy,
        }
    }

    /// `project_points` (`0x100be`) for one point: screen x, y and depth.
    fn project(&self, p: [f32; 3]) -> (i32, i32, i16) {
        let v = [
            (p[0] as f64 - self.eye[0] as f64) as f32,
            (p[1] as f64 - self.eye[1] as f64) as f32,
            (p[2] as f64 - self.eye[2] as f64) as f32,
        ];
        let m = |a: f32, b: f32| a as f64 * b as f64;
        let depth = m(v[0], self.f[0]) + (m(v[1], self.f[1]) + m(v[2], self.f[2]));
        let t = (5.0f64 / (depth as f32) as f64) as f32;
        let q = [
            (m(v[0], t) + self.o[0] as f64) as f32,
            (m(v[1], t) + self.o[1] as f64) as f32,
            (m(v[2], t) + self.o[2] as f64) as f32,
        ];
        let sx = self.cx as f64 - (m(q[0], self.r[0]) + (m(q[1], self.r[1]) + m(q[2], self.r[2])));
        let sy = self.cy as f64 - (m(q[0], self.u[0]) + (m(q[1], self.u[1]) + m(q[2], self.u[2])));
        (fistp(sx), fistp(sy), fistp(depth) as i16)
    }

    /// `0x13b50`: world-plus-38400 position to the camera-local frame used by
    /// the 3D objects (note the 15-unit shift against the ground: `base_x`
    /// is `grid_ox + frac - 15`).
    fn local_point(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        let inv = (1.0f64 / CELL as f64) as f32;
        let cell_x = chp(x as f64 * inv as f64);
        let cell_z = chp(z as f64 * inv as f64);
        let rem_x = chp(x as f64) % CELL;
        let rem_z = chp(z as f64) % CELL;
        let ax = (self.cell_x.wrapping_add(cell_x) - 0x80) as f64;
        let az = (self.cell_z.wrapping_add(cell_z) - 0x80) as f64;
        [
            (ax * CELL as f64 + self.base_x as f64 + rem_x as f64) as f32,
            y,
            (az * CELL as f64 + self.base_z as f64 + rem_z as f64) as f32,
        ]
    }
}

struct QueuedSprite {
    /// Index into `Scene::sprites`.
    src: usize,
    visible: bool,
    sx: i32,
    sy: i32,
    sy_ref: i32,
    depth: i16,
}

/// The island view renderer. Owns the back buffer, which persists between
/// frames exactly like the original's (nothing clears it; the sky and the
/// ground overdraw it).
pub struct Renderer {
    back: Vec<u8>,
    /// Sprite unpack page `0x261b88` (256 bytes per row).
    scratch: Vec<u8>,
    // Ground window (`ground_build_vertices` outputs).
    gsx: Vec<i32>,
    gsy: Vec<i32>,
    gdepth: Vec<i16>,
    gtile: Vec<u16>,
    gorder: Vec<i16>,
    // 3D object work arrays.
    osx: Vec<i32>,
    osy: Vec<i32>,
    odepth: Vec<i16>,
    /// `(first point of the instance, face index)`.
    ofaces: Vec<(usize, usize)>,
    okey: Vec<i16>,
    oorder: Vec<i16>,
    sprites: Vec<QueuedSprite>,
    sorder: Vec<i16>,
    keys: Vec<i16>,
    fog_overruns: core::cell::Cell<u32>,
    reach: Reach,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            // 640x480 like `0x4a5ae0`, filled with the border colour as
            // `view_mode_apply` does.
            back: vec![BORDER_COLOUR; PITCH * 480],
            scratch: vec![0; SCRATCH],
            gsx: vec![0; GRID * GRID + 1],
            gsy: vec![0; GRID * GRID + 1],
            gdepth: vec![0; GRID * GRID + 32],
            gtile: vec![0; GRID * GRID],
            gorder: vec![0; GRID * GRID + 1],
            osx: Vec::new(),
            osy: Vec::new(),
            odepth: Vec::new(),
            ofaces: Vec::new(),
            okey: Vec::new(),
            oorder: Vec::new(),
            sprites: Vec::new(),
            sorder: Vec::new(),
            keys: Vec::new(),
            fog_overruns: core::cell::Cell::new(0),
            reach: Reach::ORIGINAL,
        }
    }

    /// How far the view reaches ([`Reach::ORIGINAL`] unless set).
    pub fn set_reach(&mut self, reach: Reach) {
        let n = (reach.half as usize * 2).pow(2);
        self.gsx.resize(n + 1, 0);
        self.gsy.resize(n + 1, 0);
        self.gdepth.resize(n + 32, 0);
        self.gtile.resize(n, 0);
        self.gorder.resize(n + 1, 0);
        self.reach = reach;
    }

    /// The back buffer: 640 bytes per row, 480 rows, the view in its top-left
    /// corner (640x400 or 320x200).
    pub fn back(&self) -> &[u8] {
        &self.back
    }

    /// For the HUD, which the original draws into the back buffer between
    /// `ground_draw` and the present.
    pub fn back_mut(&mut self) -> &mut [u8] {
        &mut self.back
    }

    /// What `view_mode_apply` (`0x146f0`) does to the back buffer on a mode
    /// switch or island entry: fill with the border colour.
    pub fn reset(&mut self) {
        self.back.fill(BORDER_COLOUR);
    }

    /// `render_frame` up to (not including) `hud_draw`.
    pub fn render(&mut self, scene: &Scene) -> FrameInfo {
        let (w, h) = scene.mode.size();
        let clip = (0, 0, w - 1, h - 1);
        self.fog_overruns.set(0);
        let cam = Cam::new(scene, clip.2 < 0x14b);
        self.sprites_project(scene, &cam);
        self.objects_transform(scene, &cam, clip);
        self.ground_build(scene, &cam);
        self.sky_draw(scene);
        let horizon = self.ground_draw(scene, clip);
        FrameInfo { horizon, fog_overruns: self.fog_overruns.get() }
    }

    /// The present step of `render_frame`: copies the view into the screen.
    ///
    /// The original passes `width - 1` and `height - 1` to its blitters, so
    /// the last column and the last row of the view never reach the screen
    /// and keep the border colour; [`Enhancements::present_full_view`] copies
    /// them too. Mode 13h (`ViewMode::Low13h`) has no 640x480 equivalent and
    /// is shown pixel-doubled at (0, 40).
    pub fn present(&self, fb: &mut Framebuffer, mode: ViewMode, interlace: bool, enh: &Enhancements) {
        let extra = usize::from(enh.present_full_view);
        match mode {
            ViewMode::Low13h => {
                let (cw, ch) = (319 + extra, 199 + extra);
                for y in 0..ch {
                    for x in 0..cw {
                        let c = self.back[y * PITCH + x];
                        for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                            fb.pixels[(40 + y * 2 + dy) * SCREEN_W + x * 2 + dx] = c;
                        }
                    }
                }
            }
            ViewMode::LowWindow => {
                let (cw, ch) = (319 + extra, 199 + extra);
                for y in 0..ch {
                    let d = (80 + y) * SCREEN_W + 160;
                    fb.pixels[d..d + cw].copy_from_slice(&self.back[y * PITCH..y * PITCH + cw]);
                }
            }
            ViewMode::High => {
                let cw = 639 + extra;
                let rows = if interlace { 400 } else { 399 + extra };
                for y in (0..rows).step_by(if interlace { 2 } else { 1 }) {
                    let d = (40 + y) * SCREEN_W;
                    fb.pixels[d..d + cw].copy_from_slice(&self.back[y * PITCH..y * PITCH + cw]);
                }
            }
        }
    }

    // ---- world sprites: `0x13300` (sort) and `0x130f0` (project) ----

    fn sprites_project(&mut self, scene: &Scene, cam: &Cam) {
        self.sprites.clear();
        let height = |a: i32, b: i32| -> i32 {
            if (0..256).contains(&a) && (0..256).contains(&b) {
                scene.island.uni.heights[(a * 256 + b) as usize] as i32 * 3
            } else {
                0
            }
        };
        for (src, s) in scene.sprites.iter().enumerate() {
            // `world_sprite_add`: queue limits.
            if self.sprites.len() >= self.reach.max_sprites
                || s.x.wrapping_neg().wrapping_sub(scene.camera.x).wrapping_abs() >= self.reach.queue_range()
                || s.z.wrapping_neg().wrapping_sub(scene.camera.z).wrapping_abs() >= self.reach.queue_range()
            {
                continue;
            }
            let ex = s.x.wrapping_add(CELL * 0x80);
            let ez = s.z.wrapping_add(CELL * 0x80);
            let mut y = s.y / CELL;
            let (cx, rx) = (ex / CELL, ex % CELL);
            let (cz, rz) = (ez / CELL, ez % CELL);
            if y < 0 {
                let h00 = height(cx, cz);
                let a = h00 + (height(cx + 1, cz) - h00) * rx / CELL;
                let h01 = height(cx, cz + 1);
                let b = h01 + (height(cx + 1, cz + 1) - h01) * rx / CELL;
                y = a - y + (b - a) * rz / CELL;
            }
            let p = [
                (((cx - 0x80 + cam.cell_x) * CELL) as f64 + cam.base_x as f64 + rx as f64) as f32,
                y as f32,
                (((cz - 0x80 + cam.cell_z) * CELL) as f64 + cam.base_z as f64 + rz as f64) as f32,
            ];
            let (sx, sy, depth) = cam.project(p);
            // Second projection 60 height units higher: the size reference.
            let (_, sy_ref, _) = cam.project([p[0], (p[1] as f64 + 60.0) as f32, p[2]]);
            self.sprites.push(QueuedSprite { src, visible: true, sx, sy, sy_ref, depth });
        }
        let n = self.sprites.len();
        self.keys.clear();
        self.keys.extend(self.sprites.iter().map(|s| s.depth.wrapping_add(1500)));
        self.keys.push(0);
        self.sorder.clear();
        self.sorder.extend((0..=n).map(|i| i as i16));
        if n > 0 {
            quicksort(&mut self.keys, &mut self.sorder, 0, n as i32 - 1);
        }
    }

    // ---- 3D objects: `objects3d_transform` (`0x13c40`) ----

    fn objects_transform(&mut self, scene: &Scene, cam: &Cam, clip: (i32, i32, i32, i32)) {
        self.ofaces.clear();
        self.osx.clear();
        self.osy.clear();
        self.odepth.clear();
        self.okey.clear();
        self.oorder.clear();
        let (Some(mesh), true) = (scene.island.meshes.as_ref(), !scene.objects.is_empty()) else {
            return;
        };
        let queue_range = self.reach.queue_range();
        let far = self.reach.fog_far;
        // Instance positions as `object3d_place` stores them.
        let place = |o: &super::ObjectInstance| -> Option<[f32; 3]> {
            if !o.active || o.model >= mesh.objects.len() {
                return None;
            }
            let dx = chp((scene.camera.x.wrapping_neg()) as f64 - o.x as f64);
            let dz = chp((scene.camera.z.wrapping_neg()) as f64 - o.z as f64);
            if dx.wrapping_abs() >= queue_range || dz.wrapping_abs() >= queue_range {
                return None;
            }
            let off = (CELL << 7) as f64;
            Some([(o.x as f64 + off) as f32, (o.y as f64 / CELL as f64) as f32, (o.z as f64 + off) as f32])
        };
        let mut pts: Vec<[f32; 3]> = Vec::new();
        let mut any = false;
        let mut active: Vec<Option<[f32; 3]>> = Vec::with_capacity(scene.objects.len());
        for o in scene.objects {
            let mut pos = place(o);
            if let Some(p) = pos {
                let (sx, _, depth) = cam.project(cam.local_point(p[0], p[1], p[2]));
                if depth < 0x191 || i32::from(depth) > far - 1 || sx > clip.2 + 300 || sx < clip.0 - 300 {
                    pos = None;
                } else {
                    any = true;
                }
            }
            active.push(pos);
        }
        if !any {
            return;
        }
        for (o, pos) in scene.objects.iter().zip(&active) {
            let Some(pos) = *pos else { continue };
            let m = mesh.objects[o.model];
            // `0x156e0(rot_a, rot_b, 0)`: R = Rz(c) * Ry(b) * Rx(a) with c = 0.
            let (s1, c1) = ((o.rot_a as f64).sin(), (o.rot_a as f64).cos());
            let (s2, c2) = ((o.rot_b as f64).sin(), (o.rot_b as f64).cos());
            let (s3, c3) = (0f64, 1f64);
            let t1 = (c3 * s1) as f32 as f64;
            let t2 = (c3 * c1) as f32 as f64;
            let m00 = (c3 * c2) as f32 as f64;
            let m01 = (t1 * s2 - s3 * c1) as f32 as f64;
            let m02 = (t2 * s2 + s3 * s1) as f32 as f64;
            let m10 = (c2 * s3) as f32 as f64;
            let m11 = (s1 * s3 * s2 + t2) as f32 as f64;
            let m12 = (c1 * s3 * s2 - t1) as f32 as f64;
            let m20 = (-s2) as f32 as f64;
            let m21 = (c2 * s1) as f32 as f64;
            let m22 = (c2 * c1) as f32 as f64;
            let base = pts.len();
            let (ff, fc) = (m.first_face as usize, m.face_count as usize);
            for f in ff..(ff + fc).min(mesh.faces.len()) {
                self.ofaces.push((base, f));
            }
            let (pf, pc) = (m.first_point as usize, m.point_count as usize);
            for raw in &mesh.points[pf.min(mesh.points.len())..(pf + pc).min(mesh.points.len())] {
                // The loader doubles every coordinate (`0x1435d`).
                let [x, y, z] = raw.map(|v| (v as f64 * 2.0) as f32 as f64);
                let rx = (m02 * z + m01 * y + m00 * x) as f32;
                let ry = (m12 * z + m11 * y + m10 * x) as f32;
                let rz = (m22 * z + m21 * y + m20 * x) as f32;
                let wx = (pos[0] as f64 + rx as f64) as f32;
                let wy = (-(ry as f64) * 0.24 + pos[1] as f64) as f32;
                let wz = (pos[2] as f64 + rz as f64) as f32;
                pts.push(cam.local_point(wx, wy, wz));
            }
        }
        for p in &pts {
            let (sx, sy, d) = cam.project(*p);
            self.osx.push(sx);
            self.osy.push(sy);
            self.odepth.push(d);
        }
        let n = self.ofaces.len();
        for &(base, f) in &self.ofaces {
            let face = &mesh.faces[f];
            let d = |k: usize| -> i32 {
                self.odepth.get(base.wrapping_add(face.points[k] as usize)).copied().unwrap_or(0) as i32
            };
            self.okey.push(((d(2) + d(1) + d(0)) / 3) as i16 - 100);
        }
        self.keys.clear();
        self.keys.extend_from_slice(&self.okey);
        self.keys.push(0);
        self.oorder.extend((0..=n).map(|i| i as i16));
        if n > 0 {
            quicksort(&mut self.keys, &mut self.oorder, 0, n as i32 - 1);
        }
    }

    // ---- ground: `ground_build_vertices` (`0x128c0`) ----

    fn ground_build(&mut self, scene: &Scene, cam: &Cam) {
        let uni = &scene.island.uni;
        let half = self.reach.half;
        let grid = half as usize * 2;
        let x0 = (cam.grid_ox.wrapping_add(cam.frac_x) - half * CELL) as f32;
        let mut vz = (cam.grid_oz.wrapping_add(cam.frac_z) - half * CELL) as f32;
        let a0 = 0x80 - cam.cell_x - half;
        let b0 = 0x80 - cam.cell_z - half;
        let mut row_phase = scene.view.wave_phase;
        for i in 0..grid {
            row_phase = row_phase.wrapping_add(0x32);
            let b = (b0 + i as i32).clamp(0, 255) as usize;
            let mut vx = x0;
            for j in 0..grid {
                let a = (a0 + j as i32).clamp(0, 255) as usize;
                let at = a * 256 + b;
                let mut hgt = uni.heights[at] as i32 * 3;
                // 512-entry table `0x1cb3a8`: trunc(cos(k * 3.1415 * 2 / 512) * 5).
                let wave = |k: i32| chp(((k & 0x1ff) as f64 * 3.1415 * 2.0 * 0.001953125).cos() * 5.0);
                match uni.attrs[at] {
                    1 | 13 => hgt += wave((a as i32) * 128 + row_phase),
                    6 => hgt += wave((a as i32) * 128 + row_phase) * 2,
                    _ => {}
                }
                let n = i * grid + j;
                let (sx, sy, d) = cam.project([vx, hgt as f32, vz]);
                self.gsx[n] = sx;
                self.gsy[n] = sy;
                self.gdepth[n] = d;
                self.gtile[n] = uni.cells[at];
                vx += CELL as f32;
            }
            vz += CELL as f32;
        }
        let n = grid * grid;
        self.gdepth[n] = 0;
        self.keys.clear();
        self.keys.extend_from_slice(&self.gdepth[..=n]);
        for (i, o) in self.gorder.iter_mut().enumerate() {
            *o = i as i16;
        }
        quicksort(&mut self.keys, &mut self.gorder, 0, n as i32 - 1);
    }

    // ---- sky: `sky_draw` (`0x144a0`) ----

    fn sky_draw(&mut self, scene: &Scene) {
        let sky = &scene.island.sky;
        let scroll =
            (chp(scene.camera.heading as f64 * 100000.0f32 as f64) % 0x4cb26) * 200 / 100000 + 0x1900;
        let hz = scene.view.horizon;
        let back = &mut self.back;
        let mut copy = |src_row: i32, y: i32, n: usize| {
            let s = scroll as isize + src_row as isize * PITCH as isize;
            let d = y as usize * PITCH;
            for k in 0..n {
                let at = s + k as isize;
                back[d + k] = if at >= 0 { sky.get(at as usize).copied().unwrap_or(0) } else { 0 };
            }
        };
        if scene.mode == ViewMode::High {
            let (top, mid, bottom) = (hz - 0x154, hz - 0x3c, hz + 10);
            let skip = |y: i32| !(0..400).contains(&y) || (scene.interlace && y & 1 != 0);
            for y in top..=mid {
                if !skip(y) {
                    copy(y - top, y, 0x27f);
                }
            }
            for y in mid..=bottom {
                if !skip(y) {
                    self.back[y as usize * PITCH..y as usize * PITCH + 640].fill(HAZE_COLOUR);
                }
            }
        } else {
            let (top, mid, bottom) = (hz - 0xaa, hz - 0x1e, hz + 5);
            for y in top..=mid {
                if (0..200).contains(&y) {
                    copy((y - top) * 2, y, 0x13f);
                }
            }
            for y in mid..=bottom {
                if (0..200).contains(&y) {
                    self.back[y as usize * PITCH..y as usize * PITCH + 320].fill(HAZE_COLOUR);
                }
            }
        }
    }

    // ---- `ground_draw` (`0x14b60`) ----

    fn ground_draw(&mut self, scene: &Scene, clip: (i32, i32, i32, i32)) -> i32 {
        let island = scene.island;
        let reach = self.reach;
        let grid = reach.half as usize * 2;
        let fog = FogMemory { island, overruns: &self.fog_overruns };
        let fog = |level: u8, c: u8| fog.read(level, c);
        let mut t = Target {
            buf: &mut self.back,
            x0: clip.0,
            y0: clip.1,
            x1: clip.2,
            y1: clip.3,
            interlace: scene.interlace,
        };
        let fog_start = reach.fog_far - reach.fog_range;
        let mut horizon = scene.view.horizon;
        let mut face_pos = 0usize;
        let mut sprite_pos = 0usize;
        let nfaces = self.ofaces.len();
        let nsprites = self.sprites.len();
        let n = grid * grid;
        for k in 0..n {
            let idx = self.gorder[n - k] as i32;
            let (j, i) = (idx % grid as i32, idx / grid as i32);
            if j >= grid as i32 - 1 || i >= grid as i32 - 1 || idx < 0 {
                continue;
            }
            let v = idx as usize;
            let ds = [
                self.gdepth[v] as i32,
                self.gdepth[v + 1] as i32,
                self.gdepth[v + grid] as i32,
                self.gdepth[v + grid + 1] as i32,
            ];
            let dmin = ds.iter().copied().min().unwrap();
            let dmax = ds.iter().copied().max().unwrap();
            if !(dmax > 0 && dmin < reach.fog_far) {
                continue;
            }
            let tile = self.gtile[v] as i16 as i32;
            for (half, tri) in [(0, [v, v + 1, v + grid]), (1, [v + grid + 1, v + 1, v + grid])] {
                ground_triangle(&mut t, island, &self.gsx, &self.gsy, &self.gdepth, tile, half, tri, &fog, reach);
            }

            // Pending 3D object faces behind this cell.
            while face_pos < nfaces {
                let slot = self.oorder[nfaces - 1 - face_pos] as usize;
                if dmax >= self.okey[slot] as i32 {
                    break;
                }
                face_pos += 1;
                let (base, f) = self.ofaces[slot];
                let (Some(mesh), Some(map)) = (island.meshes.as_ref(), island.objmap.as_ref()) else {
                    continue;
                };
                let face = &mesh.faces[f];
                let p = face.points.map(|q| base.wrapping_add(q as usize));
                if p.iter().any(|&q| q >= self.osx.len()) {
                    continue;
                }
                let x = p.map(|q| self.osx[q]);
                let y = p.map(|q| self.osy[q]);
                let cross = x[1].wrapping_sub(x[0]).wrapping_mul(y[2].wrapping_sub(y[0]));
                let other = y[1].wrapping_sub(y[0]).wrapping_mul(x[2].wrapping_sub(x[0]));
                if cross.wrapping_sub(other) <= 0 {
                    continue;
                }
                let (lo, hi) = (x[0].min(x[1]).min(x[2]), x[0].max(x[1]).max(x[2]));
                if !(hi >= 0 && lo <= t.x1 && hi.wrapping_sub(lo) < 0x5dd) {
                    continue;
                }
                let (lo, hi) = (y[0].min(y[1]).min(y[2]), y[0].max(y[1]).max(y[2]));
                if !(hi >= 0 && lo <= t.y1 && hi.wrapping_sub(lo) < 0x5dd) {
                    continue;
                }
                if dmax < reach.fog_far + 500 {
                    let base_texel =
                        map.textures.get(face.texture as usize).map(|r| r.offset as usize).unwrap_or(0);
                    let tex = Texture { page: &map.pixels, base: base_texel };
                    tri_textured(&mut t, x, y, face.u.map(i32::from), face.v.map(i32::from), tex, true);
                    let lv = p.map(|q| (self.odepth[q] as i32 - fog_start) * 0x40 / reach.fog_range);
                    tri_fog(&mut t, x, y, lv, &fog);
                }
            }

            // Pending world sprites behind this cell.
            while sprite_pos < nsprites {
                let slot = self.sorder[nsprites - 1 - sprite_pos] as usize;
                let q = &mut self.sprites[slot];
                let depth = q.depth as i32;
                if dmax >= depth {
                    break;
                }
                sprite_pos += 1;
                if depth < reach.fog_far && q.visible && q.sx > -0x28 && q.sx <= t.x1 + 0x28 {
                    let s = &scene.sprites[q.src];
                    let span = q.sy.wrapping_sub(q.sy_ref).wrapping_abs();
                    let mut k = (span as f32 as f64 * s.scale as f64 / 200.0) as f32;
                    if s.scale < 0.0 {
                        if s.scale > -900.0 {
                            k = [0.5f32, 0.5, 1.0][scene.mode.index()];
                        } else {
                            horizon = q.sy;
                            k = 0.0;
                        }
                    }
                    let (sx, sy) = (q.sx, q.sy);
                    world_sprite_draw(&mut t, &mut self.scratch, island, s, sx, sy, k, depth, reach);
                }
                q.visible = false;
            }
        }
        horizon
    }
}

/// What a fog lookup `0x485750[level * 256 + colour]` reads. Levels 0..=31
/// are the shade tables of the UNI. The span loop does not clamp the
/// interpolated level, so slightly negative or too large levels read the
/// neighbouring globals: the tile pointer table (`0x487750`, pointers
/// `0x2c4498 + offset`) right after the tables, and the object texture page
/// (`0x48aac0`) for levels that wrapped to 0xFE/0xFF. Other bytes in that
/// range (stale table tails, the Animaps palette, the sprite queue) are not
/// modelled and read as the unchanged colour.
struct FogMemory<'a> {
    island: &'a IslandAssets,
    overruns: &'a core::cell::Cell<u32>,
}

impl FogMemory<'_> {
    #[inline]
    fn read(&self, level: u8, c: u8) -> u8 {
        let at = level as usize * 256 + c as usize;
        if at < 0x2000 {
            return self.island.uni.shade_tables[at];
        }
        self.overruns.set(self.overruns.get() + 1);
        let uni = &self.island.uni;
        let rel = at - 0x2000;
        if rel < uni.tile_offsets.len() * 4 {
            let ptr = 0x2c4498u32.wrapping_add(uni.tile_offsets[rel / 4]);
            return ptr.to_le_bytes()[rel % 4];
        }
        const OBJTEX: usize = 0x48aac0 - 0x485750;
        if at >= OBJTEX {
            if let Some(v) = self.island.objmap.as_ref().and_then(|m| m.pixels.get(at - OBJTEX)) {
                return *v;
            }
        }
        c
    }
}

/// `ground_draw_triangle` (`0x12bd0`).
#[allow(clippy::too_many_arguments)]
fn ground_triangle(
    t: &mut Target,
    island: &IslandAssets,
    sx: &[i32],
    sy: &[i32],
    depth: &[i16],
    tile: i32,
    half: i32,
    tri: [usize; 3],
    fog: &impl Fn(u8, u8) -> u8,
    reach: Reach,
) {
    let x = tri.map(|i| sx[i]);
    let y = tri.map(|i| sy[i]);
    let (lo, hi) = (x[0].min(x[1]).min(x[2]), x[0].max(x[1]).max(x[2]));
    if lo > t.x1 || hi <= t.x0 || (lo < 0 && hi > t.x1) {
        return;
    }
    let (lo, hi) = (y[0].min(y[1]).min(y[2]), y[0].max(y[1]).max(y[2]));
    if lo >= t.y1 || hi <= t.y0 || (lo < 0 && hi > t.y1) {
        return;
    }
    let cross = x[1]
        .wrapping_sub(x[0])
        .wrapping_mul(y[2].wrapping_sub(y[0]))
        .wrapping_sub(x[2].wrapping_sub(x[0]).wrapping_mul(y[1].wrapping_sub(y[0])));
    if (cross > 0 && half == 1) || (cross < 0 && half == 0) {
        return;
    }
    // Corner texels per rotation and half (`0x12dc3`..`0x12f3f`).
    const UV: [[([i32; 3], [i32; 3]); 2]; 4] = [
        [([0, 31, 0], [0, 0, 31]), ([31, 31, 0], [31, 0, 31])],
        [([0, 0, 31], [31, 0, 31]), ([31, 0, 31], [0, 0, 31])],
        [([31, 0, 31], [31, 31, 0]), ([0, 0, 31], [0, 31, 0])],
        [([31, 31, 0], [0, 31, 0]), ([0, 31, 0], [31, 31, 0])],
    ];
    let (u, v) = UV[((tile >> 13) & 3) as usize][half as usize];
    let uni = &island.uni;
    // A tile number past the offset table reads a stale pointer in the
    // original; here it shows texel 0.
    let base = uni.tile_offsets.get((tile & 0xfff) as usize).map(|o| *o as usize).unwrap_or(usize::MAX / 2);
    tri_textured(t, x, y, u, v, Texture { page: &uni.sheet, base }, false);
    let lv = tri.map(|i| (depth[i] as i32 - (reach.fog_far - reach.fog_range)) * 0x32 / reach.fog_range);
    tri_fog(t, x, y, lv, fog);
}

/// `world_sprite_draw` (`0x13500`).
#[allow(clippy::too_many_arguments)]
fn world_sprite_draw(
    t: &mut Target,
    scratch: &mut [u8],
    island: &IslandAssets,
    s: &super::WorldSprite,
    sx: i32,
    sy: i32,
    k: f32,
    depth: i32,
    reach: Reach,
) {
    let level = (depth - (reach.fog_far - reach.fog_range)) * 72 / reach.fog_range;
    if level > 0x1f {
        return;
    }
    let Some(spr) = s.bank.sprites.get(s.index) else { return };
    let (w, h) = (spr.width as usize, spr.height as usize);
    // `sprite_unpack` (`0x2d940`): row r goes to page row r + 1; each row is
    // cleared over (w + 7) / 4 dwords first. Page row 0 is never written.
    for r in 0..h {
        let at = (r + 1) * 256;
        let clear = ((w + 7) >> 2) * 4;
        if at + clear.max(w) > scratch.len() {
            break;
        }
        scratch[at..at + clear].fill(0);
        scratch[at..at + w].copy_from_slice(&spr.pixels[r * w..r * w + w]);
    }
    // Fog remap and flash run over page rows 0..h, i.e. they miss the
    // sprite's last row (`edi` starts at the page, not one row down).
    if level >= 0 {
        let table = &island.uni.shade_tables[level as usize * 256..level as usize * 256 + 256];
        for p in scratch.chunks_exact_mut(256).take(h).flat_map(|row| &mut row[..w.min(256)]) {
            if *p != 0 {
                *p = table[*p as usize];
            }
        }
    }
    if s.flags & SPRITE_FLASH != 0 {
        for p in scratch.chunks_exact_mut(256).take(h).flat_map(|row| &mut row[..w.min(256)]) {
            if *p != 0 {
                *p = 0xff;
            }
        }
    }
    let (w, h) = (w as i32, h as i32);
    let (hot_x, hot_y) = (spr.hot_x as i32, spr.hot_y as i32);
    let (mut sx, mut sy) = (sx, sy);
    // Corner offsets: 0 top-left, 1 top-right, 2 bottom-right, 3 bottom-left.
    let (ox, oy) = match s.anchor {
        Anchor::HotSpot => ([hot_x, w + hot_x, w + hot_x, hot_x], [hot_y, hot_y, h + hot_y, h + hot_y]),
        Anchor::Centre => {
            sx = sx.wrapping_add(w / 2 + hot_x);
            sy = sy.wrapping_add(h / 2 + hot_y);
            ([(-w) / 2, w / 2, w / 2, (-w) / 2], [(-h) / 2, (-h) / 2, h / 2, h / 2])
        }
        Anchor::BottomCentre => {
            sx = sx.wrapping_add(w / 2 + hot_x);
            sy = sy.wrapping_add(h + hot_y);
            ([(-w) / 2, w / 2, w / 2, (-w) / 2], [-h, -h, 0, 0])
        }
    };
    let (cs, sn) = ((s.angle as f64).cos() as f32 as f64, (s.angle as f64).sin() as f32 as f64);
    let mut x = [0i32; 4];
    let mut y = [0i32; 4];
    for c in 0..4 {
        let across = cs * ox[c] as f64 - sn * oy[c] as f64;
        let down = ox[c] as f64 * sn + oy[c] as f64 * cs;
        x[c] = if s.flags & SPRITE_MIRROR == 0 {
            chp(sx as f64 + across * k as f64)
        } else {
            chp(sx as f64 - across * k as f64)
        };
        y[c] = chp(sy as f64 + down * k as f64);
    }
    let tex = Texture { page: scratch, base: 0 };
    tri_textured(t, [x[0], x[1], x[3]], [y[0], y[1], y[3]], [0, w, 0], [0, 0, h], tex, true);
    tri_textured(t, [x[1], x[2], x[3]], [y[1], y[2], y[3]], [w, w, 0], [0, h, h], tex, true);
}
