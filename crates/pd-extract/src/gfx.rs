//! `pd-extract gfx <PUNKY dir> <out dir>`: dumps pictures, sprites, sound
//! effects and texts.
//!
//! Output layout:
//! - `ron/<NAME>.png`            indexed PNG with the displayed palette
//! - `spt/<SHEET>.png`           contact sheet, hex sprite index above each cell
//! - `spt/<SHEET>/<NNN>.png`     one RGBA PNG per sprite
//! - `spt/<SHEET>/index.json`    size and hotspot per sprite, palette source
//! - `pcm/<NAME>.wav`            16-bit mono 16 kHz
//! - `txt/ILE_<n>.json`          dialogue texts, UTF-8

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use pd_formats::bnk::Bnk;
use pd_formats::pcm::{self, Pcm};
use pd_formats::ron::{self, Ron};
use pd_formats::spt::{Sprite, SpriteSheet};
use pd_formats::txt::{Language, TextFile};

type Res<T> = Result<T, Box<dyn std::error::Error>>;
type Palette = [[u8; 3]; 256];

pub fn run(args: &[String]) -> Res<()> {
    let [src, out] = args else {
        return Err("usage: pd-extract gfx <PUNKY dir> <out dir>".into());
    };
    let src = Dir::open(Path::new(src))?;
    let out = PathBuf::from(out);

    let pictures = dump_pictures(&src, &out.join("ron"))?;
    let (sheets, sprites) = dump_sprites(&src, &out.join("spt"), &pictures)?;
    let sounds = dump_sounds(&src, &out.join("pcm"))?;
    let texts = dump_texts(&src, &out.join("txt"))?;
    println!(
        "{} pictures, {sheets} sprite sheets ({sprites} sprites), {sounds} sounds, {texts} text files -> {}",
        pictures.len(),
        out.display()
    );
    Ok(())
}

/// The source directory with case-insensitive file lookup.
struct Dir {
    files: BTreeMap<String, PathBuf>,
}

impl Dir {
    fn open(path: &Path) -> Res<Self> {
        let mut files = BTreeMap::new();
        for e in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
            let e = e?;
            if e.file_type()?.is_file() {
                files.insert(e.file_name().to_string_lossy().to_uppercase(), e.path());
            }
        }
        Ok(Self { files })
    }

    fn read(&self, name: &str) -> Res<Vec<u8>> {
        let p = self.files.get(&name.to_uppercase()).ok_or_else(|| format!("{name}: not found"))?;
        Ok(fs::read(p)?)
    }

    fn with_ext(&self, ext: &str) -> Vec<String> {
        self.files.keys().filter(|n| n.ends_with(ext)).cloned().collect()
    }
}

// ---------------------------------------------------------------- pictures

/// Returns the stored (8-bit) palette of every picture by upper-case name.
fn dump_pictures(src: &Dir, out: &Path) -> Res<BTreeMap<String, Palette>> {
    fs::create_dir_all(out)?;
    let mut all: Vec<(String, Vec<u8>)> = Vec::new();
    let bank = src.read("PICTURE.BNK")?;
    let bnk = Bnk::parse(&bank)?;
    for (name, bytes) in bnk.iter() {
        all.push((name.to_uppercase(), bytes.to_vec()));
    }
    for name in src.with_ext(".RON") {
        let bytes = src.read(&name)?;
        match all.iter().find(|(n, _)| *n == name) {
            Some((_, b)) if *b == bytes => {}
            Some(_) => all.push((format!("LOOSE_{name}"), bytes)),
            None => all.push((name, bytes)),
        }
    }
    let mut palettes = BTreeMap::new();
    for (name, bytes) in &all {
        let pic: Ron = ron::decode(bytes).map_err(|e| format!("{name}: {e}"))?;
        if !pic.complete {
            eprintln!("warning: {name}: pixel stream ended early");
        }
        write_indexed_png(
            &out.join(format!("{name}.png")),
            pic.width as u32,
            pic.height as u32,
            &pic.pixels,
            &pic.palette_rgb8(),
        )?;
        palettes.insert(name.clone(), pic.palette);
    }
    Ok(palettes)
}

// ----------------------------------------------------------------- sprites

/// `ILE_n.COL`: 8-byte header, then 256 RGB triples with 8 bits per
/// component. The game shifts them to 6 bits like a `.RON` palette
/// (`P.EXE` `0x33440`).
fn col_palette(src: &Dir, island: u8) -> Res<Palette> {
    let name = format!("ILE_{island}.COL");
    let data = src.read(&name)?;
    let raw = data.get(8..8 + 768).ok_or_else(|| format!("{name}: too short"))?;
    let mut pal = [[0u8; 3]; 256];
    for (p, c) in pal.iter_mut().zip(raw.chunks_exact(3)) {
        *p = [c[0], c[1], c[2]];
    }
    Ok(pal)
}

/// Which palette is active when the game draws a sheet, as a description
/// plus the stored 8-bit palette. See `docs/re/formats/spt.md`.
fn sheet_palette(src: &Dir, pictures: &BTreeMap<String, Palette>, sheet: &str) -> Res<(String, Palette)> {
    let stem = sheet.trim_end_matches(".SPT");
    let picture = |name: &str| -> Res<(String, Palette)> {
        let pal = pictures.get(name).ok_or_else(|| format!("{name}: needed for the palette of {sheet}"))?;
        Ok((name.to_string(), *pal))
    };
    match stem {
        // Menu fonts, drawn over MENUSPIC.RON.
        "COOLFNT2" | "COOLFNT3" => picture("MENUSPIC.RON"),
        // Installer fonts; COOLFONT is not referenced by P.EXE and uses the same colours.
        "INSTFNT1" | "INSTFNT2" | "INSTFNT3" | "COOLFONT" => picture("INSTALL.RON"),
        // Shop cursor, drawn over MARCHAND.RON.
        "HAND" => picture("MARCHAND.RON"),
        _ => {
            // Island sheets use ILE_n.COL. Sheets shared by all islands
            // (DUCK, NUMBERS, BOOM, COOLFNT1) only use indices that are
            // identical in all five island palettes, so island 1 serves.
            let island = match stem.as_bytes().last() {
                Some(d @ b'1'..=b'5') => d - b'0',
                _ => 1,
            };
            Ok((format!("ILE_{island}.COL"), col_palette(src, island)?))
        }
    }
}

fn dump_sprites(src: &Dir, out: &Path, pictures: &BTreeMap<String, Palette>) -> Res<(usize, usize)> {
    fs::create_dir_all(out)?;
    let mut all: Vec<(String, Vec<u8>)> = Vec::new();
    let bank = src.read("BANQUE.BNK")?;
    let bnk = Bnk::parse(&bank)?;
    for (name, bytes) in bnk.iter() {
        all.push((name.to_uppercase(), bytes.to_vec()));
    }
    for name in src.with_ext(".SPT") {
        let bytes = src.read(&name)?;
        match all.iter().find(|(n, _)| *n == name) {
            Some((_, b)) if *b == bytes => {}
            Some(_) => all.push((format!("LOOSE_{name}"), bytes)),
            None => all.push((name, bytes)),
        }
    }

    let mut total = 0;
    for (name, bytes) in &all {
        let sheet = SpriteSheet::parse(bytes).map_err(|e| format!("{name}: {e}"))?;
        let (pal_name, stored) = sheet_palette(src, pictures, name.trim_start_matches("LOOSE_"))?;
        let pal = ron::displayed_palette(&stored);
        let stem = name.trim_end_matches(".SPT");
        let dir = out.join(stem);
        fs::create_dir_all(&dir)?;

        let mut sprites = Vec::with_capacity(sheet.len());
        let mut json = String::new();
        writeln!(json, "{{\n  \"sheet\": {},\n  \"palette\": {},\n  \"sprites\": [", js(name), js(&pal_name))?;
        for i in 0..sheet.len() {
            let s = sheet.decode(i).map_err(|e| format!("{name}: {e}"))?;
            if s.width > 0 && s.height > 0 {
                write_rgba_png(&dir.join(format!("{i:03}.png")), s.width as u32, s.height as u32, &sprite_rgba(&s, &pal))?;
            }
            let sep = if i + 1 < sheet.len() { "," } else { "" };
            writeln!(
                json,
                "    {{\"index\": {i}, \"width\": {}, \"height\": {}, \"hot_x\": {}, \"hot_y\": {}}}{sep}",
                s.width, s.height, s.hot_x, s.hot_y
            )?;
            sprites.push(s);
        }
        json.push_str("  ]\n}\n");
        fs::write(dir.join("index.json"), json)?;

        let (w, h, rgb) = contact_sheet(&sprites, &pal);
        write_rgb_png(&out.join(format!("{stem}.png")), w, h, &rgb)?;
        total += sprites.len();
    }
    Ok((all.len(), total))
}

fn sprite_rgba(s: &Sprite, pal: &Palette) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.pixels.len() * 4);
    for (&p, &m) in s.pixels.iter().zip(&s.mask) {
        let c = pal[p as usize];
        out.extend_from_slice(&if m { [c[0], c[1], c[2], 255] } else { [0, 0, 0, 0] });
    }
    out
}

/// 3x5 hex digits for the contact sheet labels, one row per byte, MSB left.
const DIGITS: [[u8; 5]; 16] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 2, 2, 2],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
    [7, 5, 7, 5, 5],
    [6, 5, 6, 5, 6],
    [7, 4, 4, 4, 7],
    [6, 5, 5, 5, 6],
    [7, 4, 7, 4, 7],
    [7, 4, 7, 4, 4],
];

/// All sprites of a sheet in a 16-column grid on a grey checkerboard, each
/// cell labelled with the sprite index in hex.
fn contact_sheet(sprites: &[Sprite], pal: &Palette) -> (u32, u32, Vec<u8>) {
    const COLS: usize = 16;
    const LABEL: usize = 7;
    let cw = sprites.iter().map(|s| s.width as usize).max().unwrap_or(0).max(12) + 2;
    let ch = sprites.iter().map(|s| s.height as usize).max().unwrap_or(0) + LABEL + 2;
    let rows = sprites.len().div_ceil(COLS).max(1);
    let (w, h) = (cw * COLS, ch * rows);
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let dark = ((x / 8) + (y / 8)) % 2 == 0;
            let v = if x % cw == 0 || y % ch == 0 { 0x20 } else if dark { 0x58 } else { 0x68 };
            rgb[(y * w + x) * 3..][..3].fill(v);
        }
    }
    let mut put = |x: usize, y: usize, c: [u8; 3]| rgb[(y * w + x) * 3..][..3].copy_from_slice(&c);
    for (i, s) in sprites.iter().enumerate() {
        let (ox, oy) = ((i % COLS) * cw + 1, (i / COLS) * ch + 1);
        for (d, shift) in [(i >> 8) & 15, (i >> 4) & 15, i & 15].into_iter().zip([0usize, 4, 8]) {
            for (ry, bits) in DIGITS[d].iter().enumerate() {
                for rx in 0..3 {
                    if bits >> (2 - rx) & 1 == 1 {
                        put(ox + shift + rx, oy + ry, [255, 255, 255]);
                    }
                }
            }
        }
        for y in 0..s.height as usize {
            for x in 0..s.width as usize {
                let at = y * s.width as usize + x;
                if s.mask[at] {
                    put(ox + x, oy + LABEL + y, pal[s.pixels[at] as usize]);
                }
            }
        }
    }
    (w as u32, h as u32, rgb)
}

// ------------------------------------------------------------------ sounds

fn dump_sounds(src: &Dir, out: &Path) -> Res<usize> {
    fs::create_dir_all(out)?;
    let bank = src.read("SAMPLE.BNK")?;
    let bnk = Bnk::parse(&bank)?;
    for (name, bytes) in bnk.iter() {
        let sound = Pcm::parse(bytes).map_err(|e| format!("{name}: {e}"))?;
        let stem = name.to_uppercase();
        let stem = stem.trim_end_matches(".PCM");
        fs::write(out.join(format!("{stem}.wav")), wav(&sound))?;
    }
    Ok(bnk.len())
}

/// Canonical 44-byte-header PCM WAV.
fn wav(sound: &Pcm) -> Vec<u8> {
    let data_len = (sound.samples.len() * 2) as u32;
    let block = pcm::CHANNELS * 2;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&pcm::CHANNELS.to_le_bytes());
    v.extend_from_slice(&pcm::SAMPLE_RATE.to_le_bytes());
    v.extend_from_slice(&(pcm::SAMPLE_RATE * block as u32).to_le_bytes());
    v.extend_from_slice(&block.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in &sound.samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

// ------------------------------------------------------------------- texts

fn dump_texts(src: &Dir, out: &Path) -> Res<usize> {
    fs::create_dir_all(out)?;
    let mut count = 0;
    for name in src.with_ext(".TXT") {
        if !name.starts_with("ILE_") {
            continue;
        }
        let file = TextFile::parse(&src.read(&name)?).map_err(|e| format!("{name}: {e}"))?;
        let mut json = String::from("[\n");
        for (i, e) in file.entries.iter().enumerate() {
            writeln!(json, "  {{\n    \"id\": {},", e.id)?;
            for (l, lang) in Language::ALL.into_iter().enumerate() {
                let strings: Vec<String> = e.strings(lang).iter().map(|s| js(s)).collect();
                let sep = if l + 1 < Language::ALL.len() { "," } else { "" };
                writeln!(json, "    \"{}\": [{}]{sep}", lang.code(), strings.join(", "))?;
            }
            json.push_str(if i + 1 < file.entries.len() { "  },\n" } else { "  }\n" });
        }
        json.push_str("]\n");
        fs::write(out.join(format!("{}.json", name.trim_end_matches(".TXT"))), json)?;
        count += 1;
    }
    Ok(count)
}

/// JSON string literal.
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// --------------------------------------------------------------------- PNG

fn write_png(path: &Path, w: u32, h: u32, color: png::ColorType, palette: Option<&Palette>, data: &[u8]) -> Res<()> {
    let file = BufWriter::new(fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    if let Some(pal) = palette {
        enc.set_palette(pal.as_flattened().to_vec());
    }
    let mut writer = enc.write_header()?;
    writer.write_image_data(data)?;
    writer.finish()?;
    Ok(())
}

fn write_indexed_png(path: &Path, w: u32, h: u32, pixels: &[u8], pal: &Palette) -> Res<()> {
    write_png(path, w, h, png::ColorType::Indexed, Some(pal), pixels)
}

fn write_rgb_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> Res<()> {
    write_png(path, w, h, png::ColorType::Rgb, None, rgb)
}

fn write_rgba_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Res<()> {
    write_png(path, w, h, png::ColorType::Rgba, None, rgba)
}
