//! `.RON` pictures: a GIF stream with every byte XORed with `0x55` and the
//! signature changed to `RON`.
//!
//! Spec: `docs/re/formats/ron.md`. Follows the game's loader
//! (`P.EXE` `0x2b5f0` -> `0x2b6c0` -> LZW decoder `0x2b830`).

use std::fmt;

/// Every byte of a `.RON` file is XORed with this value (`0x2b450`).
pub const XOR_KEY: u8 = 0x55;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RonError(pub String);

impl fmt::Display for RonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RON: {}", self.0)
    }
}

impl std::error::Error for RonError {}

fn err<T>(msg: impl Into<String>) -> Result<T, RonError> {
    Err(RonError(msg.into()))
}

/// A decoded picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ron {
    pub width: u16,
    pub height: u16,
    /// `width * height` palette indices, row-major.
    pub pixels: Vec<u8>,
    /// Palette exactly as stored in the file: 8 bits per component.
    /// Entries past the stored colour table are zero.
    pub palette: [[u8; 3]; 256],
    /// Number of palette entries stored in the file.
    pub palette_len: usize,
    /// `false` if the pixel stream ended early or was malformed; the game
    /// keeps whatever was decoded in that case, and so do we.
    pub complete: bool,
}

/// What the game programs into the VGA DAC for a stored component:
/// the loader shifts every palette byte right by two (`0x2b65e`).
pub fn to_vga6(c: u8) -> u8 {
    c >> 2
}

/// 8-bit value shown by a VGA DAC for a 6-bit component: bit replication
/// `(v << 2) | (v >> 4)`, so 0 -> 0 and 63 -> 255 (within one step of the
/// exact `v * 255 / 63`). A plain `v << 2` would top out at 252.
pub fn vga6_to_rgb8(v: u8) -> u8 {
    let v = v & 0x3f;
    (v << 2) | (v >> 4)
}

/// Expands a palette stored with 8 bits per component to what the game
/// displays: truncate to 6 bits, then expand like the DAC.
pub fn displayed_palette(stored: &[[u8; 3]; 256]) -> [[u8; 3]; 256] {
    let mut out = [[0u8; 3]; 256];
    for (o, s) in out.iter_mut().zip(stored) {
        *o = [vga6_to_rgb8(to_vga6(s[0])), vga6_to_rgb8(to_vga6(s[1])), vga6_to_rgb8(to_vga6(s[2]))];
    }
    out
}

impl Ron {
    /// The 6-bit palette the game hands to the VGA DAC.
    pub fn palette_vga6(&self) -> [[u8; 3]; 256] {
        let mut out = self.palette;
        for c in out.iter_mut().flatten() {
            *c = to_vga6(*c);
        }
        out
    }

    /// The palette as displayed, expanded to 8 bits per component.
    pub fn palette_rgb8(&self) -> [[u8; 3]; 256] {
        displayed_palette(&self.palette)
    }

    /// Pixels expanded to RGB with the displayed palette.
    pub fn to_rgb8(&self) -> Vec<u8> {
        let pal = self.palette_rgb8();
        self.pixels.iter().flat_map(|&p| pal[p as usize]).collect()
    }
}

/// Removes the XOR layer. The result is a GIF stream whose first three
/// bytes read `RON` instead of `GIF`.
pub fn decrypt(data: &[u8]) -> Vec<u8> {
    data.iter().map(|b| b ^ XOR_KEY).collect()
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RonError> {
        if self.data.len() - self.pos < n {
            return err(format!("unexpected end of file at {:#x}", self.pos));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, RonError> {
        Ok(self.take(1)?[0])
    }
}

fn read_colour_table(r: &mut Reader, flags: u8, pal: &mut [[u8; 3]; 256]) -> Result<usize, RonError> {
    let n = 2usize << (flags & 7);
    let raw = r.take(n * 3)?;
    for (p, c) in pal.iter_mut().zip(raw.chunks_exact(3)) {
        *p = [c[0], c[1], c[2]];
    }
    Ok(n)
}

/// Decodes a `.RON` file (still XOR-encrypted, as found on the disc).
pub fn decode(data: &[u8]) -> Result<Ron, RonError> {
    let plain = decrypt(data);
    let mut r = Reader { data: &plain, pos: 0 };

    // 13-byte header: signature(3) version(3) width height flags bg aspect.
    // The game compares only the first three bytes against "RON".
    let head = r.take(13)?;
    if &head[..3] != b"RON" {
        return err("bad signature");
    }
    let mut palette = [[0u8; 3]; 256];
    let mut palette_len = 0;
    if head[10] & 0x80 != 0 {
        palette_len = read_colour_table(&mut r, head[10], &mut palette)?;
    }

    loop {
        match r.u8()? {
            0x2c => break,
            0x21 => {
                // Extension: label byte, then sub-blocks until a zero length.
                // No shipped file has one. (The original's skipper at
                // 0x2b680 treats the label as a length; a standard skip is
                // used here instead.)
                r.u8()?;
                loop {
                    let n = r.u8()? as usize;
                    if n == 0 {
                        break;
                    }
                    r.take(n)?;
                }
            }
            0 => {}
            b => return err(format!("unexpected block {b:#04x} at {:#x}", r.pos - 1)),
        }
    }

    // Image descriptor: left, top, width, height (u16 each), flags.
    let d = r.take(9)?;
    let width = u16::from_le_bytes([d[4], d[5]]);
    let height = u16::from_le_bytes([d[6], d[7]]);
    let flags = d[8];
    if width == 0 || height == 0 {
        return err("empty image");
    }
    if flags & 0x80 != 0 {
        // A local colour table overwrites the same palette buffer.
        palette_len = read_colour_table(&mut r, flags, &mut palette)?.max(palette_len);
    }
    let code_size = r.u8()?;
    if !(2..=8).contains(&code_size) {
        return err(format!("LZW code size {code_size} out of range"));
    }

    let (pixels, complete) =
        lzw_decode(&mut r, code_size, width as usize, height as usize, flags & 0x40 != 0);
    Ok(Ron { width, height, pixels, palette, palette_len, complete })
}

/// Row sink reproducing the row stepping at `0x2bb23`: full lines are
/// copied to the destination, with the four GIF interlace passes if the
/// descriptor's bit 6 is set.
struct Rows {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    line: Vec<u8>,
    row: usize,
    pass: usize,
    interlaced: bool,
    rows_done: usize,
}

const PASS_STEP: [usize; 4] = [8, 8, 4, 2]; // table at 0x41a50
const PASS_START: [usize; 4] = [0, 4, 2, 1]; // table at 0x41a5a holds 4, 2, 1

impl Rows {
    /// Returns `false` once the picture is full.
    fn push(&mut self, px: u8) -> bool {
        if self.rows_done >= self.height {
            return false;
        }
        self.line.push(px);
        if self.line.len() < self.width {
            return true;
        }
        if self.row < self.height {
            let at = self.row * self.width;
            self.pixels[at..at + self.width].copy_from_slice(&self.line);
        }
        self.line.clear();
        self.rows_done += 1;
        if self.interlaced {
            self.row += PASS_STEP[self.pass];
            while self.row >= self.height && self.pass < 3 {
                self.pass += 1;
                self.row = PASS_START[self.pass];
            }
        } else {
            self.row += 1;
        }
        true
    }
}

/// GIF LZW, variable code width up to 12 bits, codes packed LSB first in
/// length-prefixed sub-blocks. Returns the pixels and whether the stream
/// filled the picture cleanly.
fn lzw_decode(r: &mut Reader, code_size: u8, width: usize, height: usize, interlaced: bool) -> (Vec<u8>, bool) {
    let mut rows = Rows {
        pixels: vec![0; width * height],
        width,
        height,
        line: Vec::with_capacity(width),
        row: 0,
        pass: 0,
        interlaced,
        rows_done: 0,
    };

    let clear: u16 = 1 << code_size;
    let eoi = clear + 1;
    let mut next = clear + 2;
    let mut bits = code_size as u32 + 1;
    let mut limit: u32 = 1 << bits;
    let mut prev: Option<u16> = None;
    let mut first: u8 = 0;

    let mut prefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut stack: Vec<u8> = Vec::with_capacity(4096);

    // Bit reader over the sub-block chain.
    let mut block: &[u8] = &[];
    let mut acc: u32 = 0;
    let mut nacc: u32 = 0;

    let ok = 'outer: loop {
        while nacc < bits {
            if block.is_empty() {
                let Ok(n) = r.u8() else { break 'outer false };
                if n == 0 {
                    break 'outer false;
                }
                let Ok(b) = r.take(n as usize) else { break 'outer false };
                block = b;
            }
            acc |= (block[0] as u32) << nacc;
            block = &block[1..];
            nacc += 8;
        }
        let code = (acc & (limit - 1)) as u16;
        acc >>= bits;
        nacc -= bits;

        if code == eoi {
            break true;
        }
        if code > next {
            break false;
        }
        if code == clear {
            next = clear + 2;
            bits = code_size as u32 + 1;
            limit = 1 << bits;
            prev = None;
            continue;
        }

        let mut cur = code;
        stack.clear();
        if code == next {
            // Code not yet in the table: previous string + its first pixel.
            let Some(p) = prev else { break true };
            stack.push(first);
            cur = p;
        }
        while cur >= clear {
            stack.push(suffix[cur as usize]);
            cur = prefix[cur as usize];
        }
        first = cur as u8;
        stack.push(first);
        while let Some(px) = stack.pop() {
            if !rows.push(px) {
                break 'outer true;
            }
        }

        if next < 0x1000 {
            if let Some(p) = prev {
                prefix[next as usize] = p;
                suffix[next as usize] = first;
                next += 1;
                if next as u32 >= limit && bits < 12 {
                    bits += 1;
                    limit = 1 << bits;
                }
            }
        }
        prev = Some(code);
    };

    let complete = ok && rows.rows_done == height;
    (rows.pixels, complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest valid stream: 2x2 picture, 2-colour table, codes
    /// clear, 0, 1, 1 at 3 bits, then 0, end at 4 bits (the table reaches
    /// 8 entries after the third pixel code).
    fn tiny() -> Vec<u8> {
        let mut v = b"RON96a".to_vec();
        v.extend_from_slice(&[2, 0, 2, 0, 0x80, 0, 0]);
        v.extend_from_slice(&[0, 0, 0, 255, 128, 7]);
        v.push(0x2c);
        v.extend_from_slice(&[0, 0, 0, 0, 2, 0, 2, 0, 0]);
        v.push(2);
        let codes = [(4u32, 3), (0, 3), (1, 3), (1, 3), (0, 4), (5, 4)];
        let mut acc = 0u32;
        let mut n = 0;
        for (c, bits) in codes {
            acc |= c << n;
            n += bits;
        }
        v.extend_from_slice(&[3, acc as u8, (acc >> 8) as u8, (acc >> 16) as u8, 0, 0x3b]);
        v.iter().map(|b| b ^ XOR_KEY).collect()
    }

    #[test]
    fn decodes_tiny() {
        let ron = decode(&tiny()).unwrap();
        assert_eq!((ron.width, ron.height), (2, 2));
        assert_eq!(ron.pixels, [0, 1, 1, 0]);
        assert!(ron.complete);
        assert_eq!(ron.palette_len, 2);
        assert_eq!(ron.palette[1], [255, 128, 7]);
        assert_eq!(ron.palette_vga6()[1], [63, 32, 1]);
        assert_eq!(ron.palette_rgb8()[1], [255, 130, 4]);
        assert_eq!(ron.to_rgb8()[3..6], [255, 130, 4]);
    }

    #[test]
    fn dac_expansion() {
        assert_eq!((vga6_to_rgb8(0), vga6_to_rgb8(63)), (0, 255));
        for v in 0..64u8 {
            let exact = v as f32 * 255.0 / 63.0;
            assert!((vga6_to_rgb8(v) as f32 - exact).abs() < 1.0);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"").is_err());
        assert!(decode(&[0u8; 64]).is_err());
        let mut t = tiny();
        t[0] ^= 1;
        assert!(decode(&t).is_err());
    }

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    #[test]
    fn disc_pictures() {
        let Some(menu) = disc("MENUSPIC.RON") else {
            eprintln!("skipping: disc files not present");
            return;
        };
        let ron = decode(&menu).unwrap();
        assert_eq!((ron.width, ron.height), (640, 480));
        assert!(ron.complete);
        assert_eq!(ron.palette_len, 256);
        assert_eq!(ron.pixels.len(), 640 * 480);
        // A real picture uses many colours and is not constant.
        let mut used = [false; 256];
        ron.pixels.iter().for_each(|&p| used[p as usize] = true);
        assert!(used.iter().filter(|&&u| u).count() > 100);

        let bank = disc("PICTURE.BNK").unwrap();
        let bnk = crate::bnk::Bnk::parse(&bank).unwrap();
        for (name, bytes) in bnk.iter() {
            let ron = decode(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(ron.complete, "{name}");
            let expect = match name {
                "ANIMAPS.RON" => (192, 352),
                n if n.starts_with("CIEL_") => (640, 300),
                _ => (640, 480),
            };
            assert_eq!((ron.width, ron.height), expect, "{name}");
        }
        // The loose MENUSPIC.RON is a copy of the archive member.
        assert_eq!(bnk.get("MENUSPIC.RON"), Some(&menu[..]));
    }
}
