//! Triangle fillers, ported instruction by instruction from the hand-written
//! assembly of `P.EXE` (`docs/re/engine/renderer.md`, "Triangle fillers").
//!
//! - [`tri_textured`]: `0x2da30` (opaque, span `0x2e0e4`) and `0x2c838`
//!   (texel 0 transparent, span `0x2ceec`); the two differ only in the pixel rule.
//! - [`tri_fog`]: `0x2d158` (span `0x2d717`).
//!
//! All arithmetic is 32-bit two's complement with wrap-around, divisions are
//! `cdq; idiv` (truncating). Row addresses always use a pitch of 640, also in
//! the 320x200 view (`imul edi,edi,0x280`).

/// Row pitch of the back buffer (`0x280`).
pub const PITCH: usize = 640;

/// Draw target: the back buffer (`0x531574` → `0x4a5ae0`) with the inclusive
/// clip rectangle of `set_clip` (`0x41dcc/d0/d4/d8`).
pub struct Target<'a> {
    pub buf: &'a mut [u8],
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// `0x40054`: draw even rows only.
    pub interlace: bool,
}

/// A 256-byte-per-row texture page with the offset of the texture's first texel
/// (`0x4f0ee8`). Texels outside the page read 0.
#[derive(Clone, Copy)]
pub struct Texture<'a> {
    pub page: &'a [u8],
    pub base: usize,
}

impl Texture<'_> {
    #[inline]
    fn texel(&self, u8_8: i32, v8_8: i32) -> u8 {
        // `mov al,ch / mov ah,dh / mov bl,[esi+eax]`: 8 integer bits of each.
        let u = ((u8_8 >> 8) & 0xff) as usize;
        let v = ((v8_8 >> 8) & 0xff) as usize;
        self.page.get(self.base + v * 256 + u).copied().unwrap_or(0)
    }
}

#[inline]
fn idiv(a: i32, b: i32) -> i32 {
    if b == 0 { 0 } else { a.wrapping_div(b) }
}

#[inline]
fn fix(v: i32) -> i32 {
    // `shl eax,0x10 / add eax,0x8000`
    (v << 16).wrapping_add(0x8000)
}

#[inline]
fn slope(d: i32, n: i32) -> i32 {
    // `shl eax,0x10 / cdq / idiv n`
    idiv(d << 16, n)
}

/// The original's three-compare vertex sort by y. Calls `swap(i, j)` for each
/// exchange so the caller can permute its attribute arrays the same way.
fn sort_y(y: &mut [i32; 3], mut swap: impl FnMut(usize, usize)) {
    let (a, b, c) = (y[0], y[1], y[2]);
    let (mut ebx, mut ecx) = (b, c);
    if b < c {
        if b < a {
            y.swap(0, 1);
            swap(0, 1);
            ebx = a;
        }
    } else if c < a {
        y.swap(0, 2);
        swap(0, 2);
        ecx = a;
    }
    if ebx >= ecx {
        y.swap(1, 2);
        swap(1, 2);
    }
}

struct TexEdges {
    l: i32,
    dl: i32,
    r: i32,
    dr: i32,
    u: i32,
    du: i32,
    v: i32,
    dv: i32,
    dudx: i32,
    dvdx: i32,
}

/// Vertical clipping shared by the three span routines. Returns the first row
/// and the row count, after advancing the edge accumulators through `skip`.
fn clip_rows(t: &Target, y: i32, rows: i32, mut skip: impl FnMut(i32)) -> Option<(i32, i32)> {
    let (mut y, mut rows) = (y, rows);
    if y > t.y1 {
        return None;
    }
    if y < t.y0 {
        let mut n = t.y0.wrapping_sub(y);
        rows = rows.wrapping_sub(n);
        if rows < 0 {
            n = n.wrapping_add(rows);
        }
        skip(n);
        if rows <= 0 {
            return None;
        }
        y = t.y0;
    }
    let end = y.wrapping_add(rows);
    if end > t.y1 {
        // The row `y1` itself is never drawn when a span reaches it.
        rows -= end - t.y1;
        if rows <= 0 {
            return None;
        }
    }
    Some((y, rows))
}

fn tex_span(t: &mut Target, e: &mut TexEdges, tex: Texture, keyed: bool, y: i32, rows: i32) {
    let Some((y, rows)) = clip_rows(t, y, rows, |n| {
        e.l = e.l.wrapping_add(e.dl.wrapping_mul(n));
        e.r = e.r.wrapping_add(e.dr.wrapping_mul(n));
        e.u = e.u.wrapping_add(e.du.wrapping_mul(n));
        e.v = e.v.wrapping_add(e.dv.wrapping_mul(n));
    }) else {
        return;
    };
    let du8 = e.dudx >> 8;
    let dv8 = e.dvdx >> 8;
    let (mut l, mut r, mut u, mut v) = (e.l, e.r, e.u, e.v);
    for row in y..y + rows {
        if !t.interlace || row & 1 == 0 {
            let mut xl = l >> 16;
            let mut xr = r >> 16;
            let (mut uu, mut vv) = (u, v);
            if xl < t.x0 {
                let n = t.x0.wrapping_sub(xl);
                uu = uu.wrapping_add(e.dudx.wrapping_mul(n));
                vv = vv.wrapping_add(e.dvdx.wrapping_mul(n));
                xl = t.x0;
            }
            if xr > t.x1 {
                xr = t.x1;
            }
            uu >>= 8;
            vv >>= 8;
            let n = xr.wrapping_sub(xl);
            if n > 0 {
                let at = row as usize * PITCH + xl as usize;
                let dst = &mut t.buf[at..=at + n as usize];
                let mut last = 0u8;
                for d in &mut dst[..n as usize] {
                    last = tex.texel(uu, vv);
                    uu = uu.wrapping_add(du8);
                    vv = vv.wrapping_add(dv8);
                    if !keyed || last != 0 {
                        *d = last;
                    }
                }
                // `mov [edi],al` after the loop: one more pixel at `xr`,
                // repeating the last texel fetched.
                if !keyed || last != 0 {
                    dst[n as usize] = last;
                }
            }
        }
        l = l.wrapping_add(e.dl);
        r = r.wrapping_add(e.dr);
        u = u.wrapping_add(e.du);
        v = v.wrapping_add(e.dv);
    }
    e.l = l;
    e.r = r;
}

/// Affine textured triangle. `keyed` selects `tri_textured_keyed` (texel 0 is
/// not written). `x`, `y` are screen pixels, `u`, `v` whole texels.
pub fn tri_textured(
    t: &mut Target,
    mut x: [i32; 3],
    mut y: [i32; 3],
    mut u: [i32; 3],
    mut v: [i32; 3],
    tex: Texture,
    keyed: bool,
) {
    sort_y(&mut y, |i, j| {
        x.swap(i, j);
        u.swap(i, j);
        v.swap(i, j);
    });
    if y[0] > t.y1 || y[2] < t.y0 {
        return;
    }
    let mut e = TexEdges { l: 0, dl: 0, r: 0, dr: 0, u: 0, du: 0, v: 0, dv: 0, dudx: 0, dvdx: 0 };
    if y[1] == y[0] {
        // Flat top.
        let h = y[2].wrapping_sub(y[0]);
        if h <= 0 {
            return;
        }
        if x[1] > x[0] {
            e.l = fix(x[0]);
            e.dl = slope(x[2].wrapping_sub(x[0]), h);
            e.r = fix(x[1]);
            e.dr = slope(x[2].wrapping_sub(x[1]), h);
            e.u = fix(u[0]);
            e.du = slope(u[2].wrapping_sub(u[0]), h);
            e.v = fix(v[0]);
            e.dv = slope(v[2].wrapping_sub(v[0]), h);
        } else {
            e.r = fix(x[0]);
            e.dr = slope(x[2].wrapping_sub(x[0]), h);
            e.l = fix(x[1]);
            e.dl = slope(x[2].wrapping_sub(x[1]), h);
            e.u = fix(u[1]);
            e.du = slope(u[2].wrapping_sub(u[1]), h);
            e.v = fix(v[1]);
            e.dv = slope(v[2].wrapping_sub(v[1]), h);
        }
        let w = x[0].wrapping_sub(x[1]);
        if w == 0 {
            return;
        }
        e.dudx = slope(u[0].wrapping_sub(u[1]), w);
        e.dvdx = slope(v[0].wrapping_sub(v[1]), w);
        tex_span(t, &mut e, tex, keyed, y[1], h);
        return;
    }
    let h1 = y[1].wrapping_sub(y[0]);
    let h2 = y[2].wrapping_sub(y[0]);
    if h2 <= 0 {
        return;
    }
    let short = slope(x[1].wrapping_sub(x[0]), h1);
    let long = slope(x[2].wrapping_sub(x[0]), h2);
    e.l = fix(x[0]);
    e.r = e.l;
    e.u = fix(u[0]);
    e.v = fix(v[0]);
    let mut width = long.wrapping_sub(short).wrapping_mul(h1) >> 16;
    let um = idiv(u[2].wrapping_sub(u[0]).wrapping_mul(h1), h2).wrapping_add(u[0]);
    let vm = idiv(v[2].wrapping_sub(v[0]).wrapping_mul(h1), h2).wrapping_add(v[0]);
    if long > short {
        // Long edge on the right.
        e.dl = short;
        e.dr = long;
        width = width.wrapping_add(1);
        if width == 0 {
            return;
        }
        e.dudx = slope(um.wrapping_sub(u[1]), width);
        e.dvdx = slope(vm.wrapping_sub(v[1]), width);
        e.du = slope(u[1].wrapping_sub(u[0]), h1);
        e.dv = slope(v[1].wrapping_sub(v[0]), h1);
        tex_span(t, &mut e, tex, keyed, y[0], h1);
        let h = y[2].wrapping_sub(y[1]);
        if h == 0 {
            return;
        }
        e.l = fix(x[1]);
        e.dl = slope(x[2].wrapping_sub(x[1]), h);
        e.du = slope(u[2].wrapping_sub(u[1]), h);
        e.dv = slope(v[2].wrapping_sub(v[1]), h);
        e.u = fix(u[1]);
        e.v = fix(v[1]);
        tex_span(t, &mut e, tex, keyed, y[1], h);
    } else {
        // Long edge on the left.
        e.dr = short;
        e.dl = long;
        if width == 0 {
            return;
        }
        e.dudx = slope(um.wrapping_sub(u[1]), width);
        e.dvdx = slope(vm.wrapping_sub(v[1]), width);
        e.du = slope(um.wrapping_sub(u[0]), h1);
        e.dv = slope(vm.wrapping_sub(v[0]), h1);
        tex_span(t, &mut e, tex, keyed, y[0], h1);
        let h = y[2].wrapping_sub(y[1]);
        if h == 0 {
            return;
        }
        e.r = fix(x[1]);
        e.dr = slope(x[2].wrapping_sub(x[1]), h);
        e.du = slope(u[2].wrapping_sub(um), h);
        e.dv = slope(v[2].wrapping_sub(vm), h);
        e.u = fix(um);
        e.v = fix(vm);
        tex_span(t, &mut e, tex, keyed, y[1], h);
    }
}

struct FogEdges {
    l: i32,
    dl: i32,
    r: i32,
    dr: i32,
    lev: i32,
    dlev: i32,
    dldx: i32,
}

fn fog_span(t: &mut Target, e: &mut FogEdges, fog: &impl Fn(u8, u8) -> u8, y: i32, rows: i32) {
    let Some((y, rows)) = clip_rows(t, y, rows, |n| {
        e.l = e.l.wrapping_add(e.dl.wrapping_mul(n));
        e.r = e.r.wrapping_add(e.dr.wrapping_mul(n));
        e.lev = e.lev.wrapping_add(e.dlev.wrapping_mul(n));
    }) else {
        return;
    };
    let step = e.dldx >> 8;
    let (mut l, mut r, mut lev) = (e.l, e.r, e.lev);
    for row in y..y + rows {
        if !t.interlace || row & 1 == 0 {
            let mut xl = l >> 16;
            let mut xr = r >> 16;
            let mut lv = lev;
            if xl < t.x0 {
                let n = t.x0.wrapping_sub(xl);
                lv = lv.wrapping_add(e.dldx.wrapping_mul(n));
                xl = t.x0;
            }
            if xr > t.x1 {
                xr = t.x1;
            }
            let n = xr.wrapping_sub(xl).wrapping_add(1);
            if n > 1 {
                let mut a = lv >> 8;
                let at = row as usize * PITCH + xl as usize;
                for d in &mut t.buf[at..at + n as usize] {
                    // `mov ch,ah`: the level byte is not clamped here.
                    *d = fog((a >> 8) as u8, *d);
                    a = a.wrapping_add(step);
                }
            }
        }
        l = l.wrapping_add(e.dl);
        r = r.wrapping_add(e.dr);
        lev = lev.wrapping_add(e.dlev);
    }
    e.l = l;
    e.r = r;
}

/// Fog pass over a triangle: interpolates the per-vertex shade level and
/// rewrites every covered pixel as `fog(level, pixel)`.
///
/// Vertex levels are clamped to `0..=31` on entry; the triangle is skipped
/// when all three were negative. The per-pixel level byte is whatever the
/// interpolation yields (it can leave `0..=31` by rounding), so `fog` receives
/// the raw byte and has to model what lies around the shade tables in memory.
pub fn tri_fog(
    t: &mut Target,
    mut x: [i32; 3],
    mut y: [i32; 3],
    mut lv: [i32; 3],
    fog: &impl Fn(u8, u8) -> u8,
) {
    let mut negative = 0;
    for l in &mut lv {
        if *l < 0 {
            *l = 0;
            negative += 1;
        }
        if *l > 0x1f {
            *l = 0x1f;
        }
    }
    if negative == 3 {
        return;
    }
    let min_x = x[0].min(x[1]).min(x[2]);
    let max_x = x[0].max(x[1]).max(x[2]);
    if t.x0 > max_x || t.x1 < min_x {
        return;
    }
    sort_y(&mut y, |i, j| {
        x.swap(i, j);
        lv.swap(i, j);
    });
    if y[0] > t.y1 || y[2] < t.y0 {
        return;
    }
    let mut e = FogEdges { l: 0, dl: 0, r: 0, dr: 0, lev: 0, dlev: 0, dldx: 0 };
    if y[1] == y[0] {
        let h = y[2].wrapping_sub(y[0]);
        if h <= 0 {
            return;
        }
        if x[1] > x[0] {
            e.l = fix(x[0]);
            e.dl = slope(x[2].wrapping_sub(x[0]), h);
            e.r = fix(x[1]);
            e.dr = slope(x[2].wrapping_sub(x[1]), h);
            e.lev = fix(lv[0]);
            e.dlev = slope(lv[2].wrapping_sub(lv[0]), h);
        } else {
            e.r = fix(x[0]);
            e.dr = slope(x[2].wrapping_sub(x[0]), h);
            e.l = fix(x[1]);
            e.dl = slope(x[2].wrapping_sub(x[1]), h);
            e.lev = fix(lv[1]);
            e.dlev = slope(lv[2].wrapping_sub(lv[1]), h);
        }
        let w = x[0].wrapping_sub(x[1]);
        if w == 0 {
            return;
        }
        e.dldx = slope(lv[0].wrapping_sub(lv[1]), w);
        fog_span(t, &mut e, fog, y[1], h);
        return;
    }
    let h1 = y[1].wrapping_sub(y[0]);
    let h2 = y[2].wrapping_sub(y[0]);
    if h2 <= 0 {
        return;
    }
    let short = slope(x[1].wrapping_sub(x[0]), h1);
    let long = slope(x[2].wrapping_sub(x[0]), h2);
    e.l = fix(x[0]);
    e.r = e.l;
    e.lev = fix(lv[0]);
    let mut width = long.wrapping_sub(short).wrapping_mul(h1) >> 16;
    let lm = idiv(lv[2].wrapping_sub(lv[0]).wrapping_mul(h1), h2).wrapping_add(lv[0]);
    if long > short {
        e.dl = short;
        e.dr = long;
        width = width.wrapping_add(3);
        if width == 0 {
            return;
        }
        e.dldx = slope(lm.wrapping_sub(lv[1]), width);
        e.dlev = slope(lv[1].wrapping_sub(lv[0]), h1);
        fog_span(t, &mut e, fog, y[0], h1);
        let h = y[2].wrapping_sub(y[1]);
        if h == 0 {
            return;
        }
        e.l = fix(x[1]);
        e.dl = slope(x[2].wrapping_sub(x[1]), h);
        e.dlev = slope(lv[2].wrapping_sub(lv[1]), h);
        e.lev = fix(lv[1]);
        fog_span(t, &mut e, fog, y[1], h);
    } else {
        e.dr = short;
        e.dl = long;
        width = width.wrapping_sub(2);
        if width == 0 {
            return;
        }
        e.dldx = slope(lm.wrapping_sub(lv[1]), width);
        e.dlev = slope(lm.wrapping_sub(lv[0]), h1);
        fog_span(t, &mut e, fog, y[0], h1);
        let h = y[2].wrapping_sub(y[1]);
        if h == 0 {
            return;
        }
        e.r = fix(x[1]);
        e.dr = slope(x[2].wrapping_sub(x[1]), h);
        e.dlev = slope(lv[2].wrapping_sub(lm), h);
        e.lev = fix(lm);
        fog_span(t, &mut e, fog, y[1], h);
    }
}
