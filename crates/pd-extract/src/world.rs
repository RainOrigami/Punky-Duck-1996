//! `pd-extract world <PUNKY dir> <out dir> [--px N]`
//!
//! Dumps the per-island world data: terrain (`ILE_n.UNI`) as PNG layers,
//! palette (`ILE_n.COL`), object texture page (`OBJETS_n.MAP`), 3D objects
//! (`OBJETS_n.3D`) as Wavefront OBJ, bonus and monster placements
//! (`ILE0n.TRJ`, `MONSTERn.TRJ`) as JSON and as an overlay on the terrain
//! picture, and `GAME.STP` as JSON.
//!
//! `--px N` sets the terrain picture scale in pixels per cell (1, 2, 4, 8,
//! 16 or 32; default 8 = 2048x2048, 32 = the full 8192x8192 texel grid).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use pd_formats::col::Col;
use pd_formats::mesh3d::{Mesh3d, POINT_SCALE};
use pd_formats::objmap::{ObjMap, PAGE_WIDTH};
use pd_formats::stp::{DIFFICULTIES, LANGUAGES, Stp};
use pd_formats::trj::{self, Trj};
use pd_formats::uni::{self, Uni};

type Res<T> = Result<T, Box<dyn std::error::Error>>;
type Palette = [[u8; 3]; 256];

pub fn run(args: &[String]) -> Res<()> {
    let mut px = 8usize;
    let mut pos = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--px" {
            px = it.next().ok_or("--px needs a value")?.parse()?;
        } else {
            pos.push(a.as_str());
        }
    }
    if pos.len() != 2 || !matches!(px, 1 | 2 | 4 | 8 | 16 | 32) {
        return Err("usage: pd-extract world <PUNKY dir> <out dir> [--px 1|2|4|8|16|32]".into());
    }
    let src = Path::new(pos[0]);
    let out = PathBuf::from(pos[1]);
    fs::create_dir_all(&out)?;

    let mut done = 0;
    for n in 1..=5 {
        done += island(src, &out, n, px)?;
    }
    if let Some(d) = read(src, "GAME.STP") {
        let s = Stp::parse(&d)?;
        fs::write(out.join("game_stp.json"), stp_json(&s))?;
        println!("GAME.STP -> game_stp.json");
        done += 1;
    }
    if done == 0 {
        return Err(format!("no world files found in {}", src.display()).into());
    }
    Ok(())
}

/// Reads a file from the disc directory, trying the upper-case disc name and
/// a lower-case variant. `None` when absent.
fn read(dir: &Path, name: &str) -> Option<Vec<u8>> {
    fs::read(dir.join(name))
        .or_else(|_| fs::read(dir.join(name.to_lowercase())))
        .ok()
}

fn island(src: &Path, out: &Path, n: u32, px: usize) -> Res<usize> {
    let mut done = 0;

    // Palette: the game shows the island with ILE_n.COL; the palettes
    // embedded in UNI and MAP are the same colours in 6-bit form.
    let col = match read(src, &format!("ILE_{n}.COL")) {
        Some(d) => Some(Col::parse(&d)?),
        None => None,
    };
    if let Some(c) = &col {
        write_png(&out.join(format!("ile_{n}_col.png")), 256, 256, &swatches(), &c.rgb8)?;
        fs::write(out.join(format!("ile_{n}_col.json")), col_json(c))?;
        println!("ILE_{n}.COL -> ile_{n}_col.png, ile_{n}_col.json");
        done += 1;
    }

    let mut terrain: Option<(usize, Vec<u8>, Palette)> = None;
    if let Some(d) = read(src, &format!("ILE_{n}.UNI")) {
        let u = Uni::parse(&d)?;
        let pal = col.as_ref().map(|c| c.rgb8).unwrap_or_else(|| u.palette_rgb8());
        let (side, img) = u.render(px);
        write_png(&out.join(format!("ile_{n}.png")), side, side, &img, &pal)?;
        write_png(
            &out.join(format!("ile_{n}_tiles.png")),
            uni::SHEET_WIDTH,
            u.sheet_rows,
            &u.sheet,
            &pal,
        )?;
        // Layers are stored [x][z]; pictures are row = z, column = x.
        let transpose = |layer: &[u8]| -> Vec<u8> {
            let g = uni::GRID;
            let mut o = vec![0u8; g * g];
            for x in 0..g {
                for z in 0..g {
                    o[z * g + x] = layer[x * g + z];
                }
            }
            o
        };
        let grey: Palette = std::array::from_fn(|i| [i as u8; 3]);
        // Heights are small (max 185 on disc); stretch to the full grey range.
        let hmax = u.heights.iter().copied().max().unwrap_or(0).max(1) as u32;
        let h: Vec<u8> = transpose(&u.heights)
            .iter()
            .map(|&v| (v as u32 * 255 / hmax) as u8)
            .collect();
        write_png(&out.join(format!("ile_{n}_height.png")), 256, 256, &h, &grey)?;
        write_png(
            &out.join(format!("ile_{n}_attr.png")),
            256,
            256,
            &transpose(&u.attrs),
            &attr_palette(),
        )?;
        write_png(
            &out.join(format!("ile_{n}_shade.png")),
            256,
            uni::SHADE_LEVELS,
            &u.shade_tables,
            &pal,
        )?;
        fs::write(out.join(format!("ile_{n}_uni.json")), uni_json(&u))?;
        println!(
            "ILE_{n}.UNI -> ile_{n}.png ({side}x{side}), _tiles, _height, _attr, _shade, _uni.json ({} tiles)",
            u.tile_offsets.len()
        );
        terrain = Some((side, img, pal));
        done += 1;
    }

    let map = match read(src, &format!("OBJETS_{n}.MAP")) {
        Some(d) => Some(ObjMap::parse(&d)?),
        None => None,
    };
    if let Some(m) = &map {
        let pal = col.as_ref().map(|c| c.rgb8).unwrap_or_else(|| vga_to_rgb8(&m.palette));
        write_png(&out.join(format!("objets_{n}_page.png")), PAGE_WIDTH, m.rows, &m.pixels, &pal)?;
        for (i, t) in m.textures.iter().enumerate() {
            if t.width == 0 || t.height == 0 {
                continue;
            }
            let p = m.texture_pixels(i).unwrap_or_default();
            write_png(
                &out.join(format!("objets_{n}_tex_{i:02}.png")),
                t.width as usize,
                t.height as usize,
                &p,
                &pal,
            )?;
        }
        println!(
            "OBJETS_{n}.MAP -> objets_{n}_page.png (256x{}), {} textures",
            m.rows,
            m.textures.len()
        );
        done += 1;
    }

    if let Some(d) = read(src, &format!("OBJETS_{n}.3D")) {
        let mesh = Mesh3d::parse(&d)?;
        let mut mtl = String::new();
        let tex_count = map.as_ref().map(|m| m.textures.len()).unwrap_or(0);
        for i in 0..tex_count {
            writeln!(mtl, "newmtl tex_{i:02}\nKd 1 1 1\nmap_Kd objets_{n}_tex_{i:02}.png\n")?;
        }
        fs::write(out.join(format!("objets_{n}.mtl")), mtl)?;
        for i in 0..mesh.objects.len() {
            let obj = object_obj(&mesh, map.as_ref(), n, i)?;
            fs::write(out.join(format!("objets_{n}_obj_{i:02}.obj")), obj)?;
        }
        println!(
            "OBJETS_{n}.3D -> {} OBJ files ({} points, {} faces), objets_{n}.mtl",
            mesh.objects.len(),
            mesh.points.len(),
            mesh.faces.len()
        );
        done += 1;
    }

    let bonus = match read(src, &format!("ILE0{n}.TRJ")) {
        Some(d) => Some(Trj::parse(&d)?),
        None => None,
    };
    let monsters = match read(src, &format!("MONSTER{n}.TRJ")) {
        Some(d) => Some(Trj::parse(&d)?),
        None => None,
    };
    if let Some(t) = &bonus {
        fs::write(out.join(format!("ile0{n}_trj.json")), trj_json(t, false))?;
        println!("ILE0{n}.TRJ -> ile0{n}_trj.json ({} bonuses)", t.records.len());
        done += 1;
    }
    if let Some(t) = &monsters {
        fs::write(out.join(format!("monster{n}_trj.json")), trj_json(t, true))?;
        println!("MONSTER{n}.TRJ -> monster{n}_trj.json ({} monsters)", t.records.len());
        done += 1;
    }

    if let Some((side, mut img, pal)) = terrain {
        if bonus.is_some() || monsters.is_some() {
            overlay(&mut img, side, px, &pal, bonus.as_ref(), monsters.as_ref());
            write_png(&out.join(format!("ile_{n}_overlay.png")), side, side, &img, &pal)?;
            println!("  overlay -> ile_{n}_overlay.png");
        }
    }
    Ok(done)
}

fn vga_to_rgb8(p: &Palette) -> Palette {
    let mut o = *p;
    for c in o.iter_mut() {
        for v in c.iter_mut() {
            let x = *v & 0x3f;
            *v = (x << 2) | (x >> 4);
        }
    }
    o
}

/// 16x16 grid of 16-pixel swatches, colour index = row * 16 + column.
fn swatches() -> Vec<u8> {
    let mut o = vec![0u8; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            o[y * 256 + x] = ((y / 16) * 16 + x / 16) as u8;
        }
    }
    o
}

/// Distinct colours for the small attribute values found on disc (0..=24).
fn attr_palette() -> Palette {
    let mut p = [[255u8, 0, 255]; 256];
    p[0] = [40, 120, 40]; // free land
    p[1] = [30, 60, 200]; // animated (water)
    for (i, c) in p.iter_mut().enumerate().skip(2).take(30) {
        // Spread hues; keep them bright so they stand out from 0 and 1.
        let h = (i * 67) % 360;
        let f = |o: usize| -> u8 {
            let k = ((h + o) % 360) as f32 / 60.0;
            let v = (k - 3.0).abs() - 1.0;
            (v.clamp(0.0, 1.0) * 200.0) as u8 + 55
        };
        *c = [f(0), f(240), f(120)];
    }
    p
}

// ---- overlay ---------------------------------------------------------------

fn nearest(pal: &Palette, rgb: [u8; 3]) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for (i, c) in pal.iter().enumerate() {
        let d: u32 = (0..3)
            .map(|k| {
                let d = c[k] as i32 - rgb[k] as i32;
                (d * d) as u32
            })
            .sum();
        if d < best.0 {
            best = (d, i as u8);
        }
    }
    best.1
}

struct Canvas<'a> {
    img: &'a mut [u8],
    side: usize,
    px: usize,
}

impl Canvas<'_> {
    /// World coordinate to picture pixel: cell = (w + 128*300) / 300.
    fn to_px(&self, w: i32) -> i32 {
        let half = (uni::GRID as i64 / 2) * uni::CELL_SIZE as i64;
        ((w as i64 + half) * self.px as i64).div_euclid(uni::CELL_SIZE as i64) as i32
    }

    fn put(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && (x as usize) < self.side && (y as usize) < self.side {
            self.img[y as usize * self.side + x as usize] = c;
        }
    }

    fn square(&mut self, wx: i32, wz: i32, r: i32, c: u8, border: u8) {
        let (cx, cy) = (self.to_px(wx), self.to_px(wz));
        for dy in -r..=r {
            for dx in -r..=r {
                let edge = dx.abs() == r || dy.abs() == r;
                self.put(cx + dx, cy + dy, if edge { border } else { c });
            }
        }
    }
}

/// Applies the game's waypoint arithmetic for one lap of a monster's path:
/// spawn point, then one computed waypoint per path point. This is the
/// arithmetic only (see `trj::next_waypoint`); whether a monster actually
/// reaches each waypoint in play is not established.
fn monster_lap(t: &Trj, r: &trj::Record) -> Vec<(i32, i32)> {
    let pts = t.path_points(r);
    let (start, first) = t.monster_start(r);
    let mut out = vec![start];
    if pts.len() < 2 {
        return out;
    }
    out.push(first);
    let mut cur = first;
    let mut idx = 1usize;
    for _ in 0..pts.len() {
        idx += 1;
        if idx == pts.len() {
            idx = 0;
        }
        cur = trj::next_waypoint(r, cur, pts[idx]);
        out.push(cur);
    }
    out
}

fn overlay(
    img: &mut [u8],
    side: usize,
    px: usize,
    pal: &Palette,
    bonus: Option<&Trj>,
    monsters: Option<&Trj>,
) {
    let black = nearest(pal, [0, 0, 0]);
    let white = nearest(pal, [255, 255, 255]);
    let yellow = nearest(pal, [255, 230, 0]);
    let orange = nearest(pal, [255, 128, 0]);
    let red = nearest(pal, [255, 0, 0]);
    let magenta = nearest(pal, [255, 0, 255]);
    let r = (px as i32 / 4).max(1);
    let mut cv = Canvas { img, side, px };
    if let Some(t) = bonus {
        for rec in &t.records {
            // Kinds below 24 are rare, one-off items on every island; mark
            // them differently from the common pickups.
            let c = if rec.kind < 24 { orange } else { yellow };
            cv.square(rec.x, rec.z, r, c, black);
        }
    }
    if let Some(t) = monsters {
        for rec in &t.records {
            // Spawn point only. The waypoint arithmetic of the game is in
            // `trj::next_waypoint`; its result is in the JSON (`lap_as_coded`)
            // but is not drawn because what it means in play is unconfirmed.
            let (start, _) = t.monster_start(rec);
            let moving = t.path_points(rec).len() > 1;
            cv.square(start.0, start.1, r + 1, if moving { magenta } else { red }, white);
        }
    }
}

// ---- OBJ -------------------------------------------------------------------

/// One object as Wavefront OBJ. Coordinates are the file values times the
/// loader's scale of 2, rotated half a turn about X (Y and Z negated) so
/// that +Y is up; a rotation keeps the triangle winding.
fn object_obj(mesh: &Mesh3d, map: Option<&ObjMap>, n: u32, index: usize) -> Res<String> {
    let mut s = String::new();
    writeln!(s, "# OBJETS_{n}.3D object {index}")?;
    writeln!(s, "mtllib objets_{n}.mtl")?;
    writeln!(s, "o objets_{n}_obj_{index:02}")?;
    for p in mesh.object_points(index) {
        writeln!(
            s,
            "v {} {} {}",
            p[0] * POINT_SCALE,
            -p[1] * POINT_SCALE,
            -p[2] * POINT_SCALE
        )?;
    }
    let faces = mesh.object_faces(index);
    for f in faces {
        let (w, h) = map
            .and_then(|m| m.textures.get(f.texture as usize))
            .map(|t| (t.width.max(1) as f32, t.height.max(1) as f32))
            .unwrap_or((256.0, 256.0));
        for k in 0..3 {
            writeln!(
                s,
                "vt {} {}",
                (f.u[k] as f32 + 0.5) / w,
                1.0 - (f.v[k] as f32 + 0.5) / h
            )?;
        }
    }
    let mut current = i16::MIN;
    for (i, f) in faces.iter().enumerate() {
        if f.texture != current {
            current = f.texture;
            writeln!(s, "usemtl tex_{current:02}")?;
        }
        let [a, b, c] = f.points.map(|p| p as i32 + 1);
        let t = i * 3 + 1;
        writeln!(s, "f {a}/{t} {b}/{} {c}/{}", t + 1, t + 2)?;
    }
    Ok(s)
}

// ---- JSON ------------------------------------------------------------------

fn num(v: f32) -> String {
    if v.is_finite() { format!("{v}") } else { "null".into() }
}

fn stp_json(s: &Stp) -> String {
    let name = |list: &[&str], i: u32| -> String {
        list.get(i as usize).map(|n| format!("\"{n}\"")).unwrap_or_else(|| "null".into())
    };
    format!(
        "{{\n  \"sound_on\": {},\n  \"music_on\": {},\n  \"controller\": {},\n  \"controller_name\": {},\n  \"sfx_volume\": {},\n  \"cd_volume\": {},\n  \"difficulty\": {},\n  \"difficulty_name\": {},\n  \"language\": {},\n  \"language_name\": {},\n  \"game_speed\": {}\n}}\n",
        s.sound_on,
        s.music_on,
        s.controller,
        name(&["KEYBOARD", "JOYSTICK"], s.controller),
        s.sfx_volume,
        s.cd_volume,
        s.difficulty,
        name(&DIFFICULTIES, s.difficulty),
        s.language,
        name(&LANGUAGES, s.language),
        s.game_speed
    )
}

fn col_json(c: &Col) -> String {
    let mut s = format!(
        "{{\n  \"size_field\": {},\n  \"magic\": {},\n  \"version\": {},\n  \"rgb8\": [\n",
        c.size_field, c.magic, c.version
    );
    for (i, p) in c.rgb8.iter().enumerate() {
        let sep = if i == 255 { "" } else { "," };
        let _ = writeln!(s, "    [{}, {}, {}]{sep}", p[0], p[1], p[2]);
    }
    s.push_str("  ]\n}\n");
    s
}

fn uni_json(u: &Uni) -> String {
    let mut hist = [0u32; 256];
    for &a in &u.attrs {
        hist[a as usize] += 1;
    }
    let attrs: Vec<String> = hist
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(v, n)| format!("\"{v}\": {n}"))
        .collect();
    format!(
        "{{\n  \"grid\": {},\n  \"cell_size\": {},\n  \"height_scale\": {},\n  \"tile_count\": {},\n  \"sheet_rows\": {},\n  \"max_height\": {},\n  \"attr_histogram\": {{{}}}\n}}\n",
        uni::GRID,
        uni::CELL_SIZE,
        uni::HEIGHT_SCALE,
        u.tile_offsets.len(),
        u.sheet_rows,
        u.heights.iter().copied().max().unwrap_or(0),
        attrs.join(", ")
    )
}

fn trj_json(t: &Trj, monsters: bool) -> String {
    let mut s = String::from("{\n  \"paths\": [");
    for (i, p) in t.paths.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        let _ = write!(s, "{sep}{{\"first_point\": {}, \"point_count\": {}}}", p.first_point, p.point_count);
    }
    s.push_str("],\n  \"points\": [");
    for (i, p) in t.points.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        let _ = write!(s, "{sep}[{}, {}]", num(p[0]), num(p[1]));
    }
    s.push_str("],\n  \"records\": [\n");
    for (i, r) in t.records.iter().enumerate() {
        let _ = write!(
            s,
            "    {{\"field0\": {}, \"path\": {}, \"field8\": {}, \"x\": {}, \"z\": {}, \"scale\": {}, \"angle\": {}, \"kind\": {}, \"cell\": [{}, {}]",
            r.field0,
            r.path,
            r.field8,
            r.x,
            r.z,
            num(r.scale),
            num(r.angle),
            r.kind,
            uni::world_to_cell(r.x),
            uni::world_to_cell(r.z)
        );
        if monsters {
            let lap: Vec<String> =
                monster_lap(t, r).iter().map(|p| format!("[{}, {}]", p.0, p.1)).collect();
            let _ = write!(s, ", \"lap_as_coded\": [{}]", lap.join(", "));
        }
        s.push_str(if i + 1 == t.records.len() { "}\n" } else { "},\n" });
    }
    s.push_str("  ]\n}\n");
    s
}

// ---- PNG (8-bit indexed, stored deflate blocks, no dependencies) -----------

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for chunk in chunks {
        for &b in *chunk {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
    }
    !crc
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc32(&[kind, body]).to_be_bytes());
}

fn write_png(path: &Path, w: usize, h: usize, pixels: &[u8], pal: &Palette) -> Res<()> {
    if w == 0 || h == 0 || pixels.len() < w * h {
        return Err(format!("{}: bad image size {w}x{h}", path.display()).into());
    }
    // Scanlines with filter type 0.
    let mut raw = Vec::with_capacity((w + 1) * h);
    for row in pixels[..w * h].chunks_exact(w) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    // zlib stream of stored blocks.
    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    z.extend_from_slice(&[0x78, 0x01]);
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(if blocks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in raw.chunks(5552) {
        for &v in chunk {
            a += v as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = Vec::with_capacity(z.len() + 1024);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 3, 0, 0, 0]); // 8-bit, indexed colour
    png_chunk(&mut out, b"IHDR", &ihdr);
    let plte: Vec<u8> = pal.iter().flatten().copied().collect();
    png_chunk(&mut out, b"PLTE", &plte);
    png_chunk(&mut out, b"IDAT", &z);
    png_chunk(&mut out, b"IEND", &[]);
    fs::write(path, out)?;
    Ok(())
}
