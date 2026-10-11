//! `.SPT` sprite sheets (also the `COOLFNT*` / `INSTFNT*` fonts).
//!
//! Spec: `docs/re/formats/spt.md`. Follows the game's blitters
//! (`P.EXE` `0x340e8` normal, `0x34468` mirrored) and the text routines
//! (`0x32bb0` select font, `0x32c10` measure, `0x32ce0` draw).

use std::fmt;

/// Size of one table record.
pub const ENTRY_SIZE: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SptError(pub String);

impl fmt::Display for SptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SPT: {}", self.0)
    }
}

impl std::error::Error for SptError {}

fn err<T>(msg: impl Into<String>) -> Result<T, SptError> {
    Err(SptError(msg.into()))
}

/// One record of the sprite table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SptEntry {
    /// Offset of the sprite data, relative to file offset 4 (the start of
    /// the table), not to the start of the file.
    pub offset: u32,
    /// Size of the sprite data in bytes (8-byte header + rows).
    pub size: u32,
    pub width: i16,
    pub height: i16,
    /// Added to the draw position: the blitter puts the sprite's top-left
    /// corner at `(x + hot_x, y + hot_y)`. Usually negative.
    pub hot_x: i16,
    pub hot_y: i16,
}

/// One horizontal run of opaque pixels inside a sprite row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run<'a> {
    /// Column of the first pixel, from the sprite's left edge.
    pub x: u8,
    /// Column of the first pixel when drawn mirrored
    /// (`width - 1 - x` in every shipped file); the mirrored blitter
    /// writes the run right-to-left from here.
    pub mirror_x: u8,
    pub pixels: &'a [u8],
}

/// A decoded sprite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: u16,
    pub height: u16,
    pub hot_x: i16,
    pub hot_y: i16,
    /// `width * height` palette indices, row-major. Transparent pixels are 0.
    pub pixels: Vec<u8>,
    /// `width * height`; `true` where the sprite covers the pixel.
    /// Transparency is positional (gaps between runs), not a colour key:
    /// an opaque pixel may have any index, including 0.
    pub mask: Vec<bool>,
}

/// A parsed sheet borrowing the file bytes.
#[derive(Debug, Clone)]
pub struct SpriteSheet<'a> {
    data: &'a [u8],
    entries: Vec<SptEntry>,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*d.get(at)?, *d.get(at + 1)?]))
}

impl<'a> SpriteSheet<'a> {
    /// Parses the table. Sprite data is validated lazily by [`Self::runs`]
    /// and [`Self::decode`].
    pub fn parse(data: &'a [u8]) -> Result<Self, SptError> {
        if data.len() < 4 {
            return err("file shorter than the count field");
        }
        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let table_end = count
            .checked_mul(ENTRY_SIZE)
            .and_then(|n| n.checked_add(4))
            .filter(|&n| n <= data.len())
            .ok_or_else(|| SptError(format!("table of {count} sprites exceeds file")))?;
        let entries = data[4..table_end]
            .chunks_exact(ENTRY_SIZE)
            .map(|r| SptEntry {
                offset: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                size: u32::from_le_bytes([r[4], r[5], r[6], r[7]]),
                width: i16::from_le_bytes([r[8], r[9]]),
                height: i16::from_le_bytes([r[10], r[11]]),
                hot_x: i16::from_le_bytes([r[12], r[13]]),
                hot_y: i16::from_le_bytes([r[14], r[15]]),
            })
            .collect();
        Ok(Self { data, entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[SptEntry] {
        &self.entries
    }

    /// End of the last sprite's data as a file offset.
    fn data_end(&self) -> usize {
        self.entries
            .iter()
            .map(|e| e.offset as usize + 4 + e.size as usize)
            .max()
            .unwrap_or(4 + self.entries.len() * ENTRY_SIZE)
    }

    /// The 256-colour palette appended after the last sprite in every
    /// shipped file: 768 bytes, 6-bit VGA components (0..=63).
    ///
    /// This is the palette the sheet was authored with. The game never
    /// reads it (sprites are drawn with whatever palette is active) and in
    /// several files it is stale, e.g. `ENEMY4.SPT` carries island 3's
    /// colours. Use the island's `.COL` or the screen's `.RON` palette to
    /// reproduce the game; see `docs/re/formats/spt.md`.
    pub fn trailing_palette(&self) -> Option<[[u8; 3]; 256]> {
        let raw = self.data.get(self.data_end()..)?;
        if raw.len() != 768 {
            return None;
        }
        let mut pal = [[0u8; 3]; 256];
        for (p, c) in pal.iter_mut().zip(raw.chunks_exact(3)) {
            *p = [c[0], c[1], c[2]];
        }
        Some(pal)
    }

    /// Raw sprite data (8-byte header + rows) of sprite `index`.
    pub fn sprite_data(&self, index: usize) -> Result<&'a [u8], SptError> {
        let e = self
            .entries
            .get(index)
            .ok_or_else(|| SptError(format!("sprite {index} out of range")))?;
        let start = e.offset as usize + 4;
        let end = start.checked_add(e.size as usize).filter(|&n| n <= self.data.len());
        match end {
            Some(end) if e.size >= 8 => Ok(&self.data[start..end]),
            _ => err(format!("sprite {index} data lies outside the file")),
        }
    }

    /// The rows of sprite `index`, each a list of opaque runs, exactly as
    /// the blitter walks them.
    pub fn runs(&self, index: usize) -> Result<Vec<Vec<Run<'a>>>, SptError> {
        let d = self.sprite_data(index)?;
        let bad = || SptError(format!("sprite {index}: truncated row data"));
        // Data header: width, height, hot_x, hot_y (i16 each). The blitter
        // takes only the row count from here (offset 2); the rest comes
        // from the table.
        let rows = u16_at(d, 2).ok_or_else(bad)? as usize;
        let mut pos = 8;
        let mut out = Vec::with_capacity(rows);
        for _ in 0..rows {
            let nruns = u16_at(d, pos).ok_or_else(bad)? as usize;
            let row_len = u16_at(d, pos + 2).ok_or_else(bad)? as usize;
            let row_end = pos.checked_add(row_len).filter(|&n| n <= d.len()).ok_or_else(bad)?;
            let mut p = pos + 4;
            let mut runs = Vec::with_capacity(nruns);
            for _ in 0..nruns {
                let x = *d.get(p).ok_or_else(bad)?;
                let mirror_x = *d.get(p + 1).ok_or_else(bad)?;
                let n = u16_at(d, p + 2).ok_or_else(bad)? as usize;
                let pixels = d.get(p + 4..p + 4 + n).ok_or_else(bad)?;
                runs.push(Run { x, mirror_x, pixels });
                p += 4 + n;
            }
            if p != row_end {
                return err(format!("sprite {index}: row length field disagrees with its runs"));
            }
            out.push(runs);
            pos = row_end;
        }
        Ok(out)
    }

    /// Decodes sprite `index` to indexed pixels plus a coverage mask.
    pub fn decode(&self, index: usize) -> Result<Sprite, SptError> {
        let rows = self.runs(index)?;
        let e = self.entries[index];
        if e.width < 0 || e.height < 0 {
            return err(format!("sprite {index}: negative size"));
        }
        let (w, h) = (e.width as usize, e.height as usize);
        if rows.len() != h {
            return err(format!("sprite {index}: {} rows for height {h}", rows.len()));
        }
        let mut pixels = vec![0u8; w * h];
        let mut mask = vec![false; w * h];
        for (y, runs) in rows.iter().enumerate() {
            for run in runs {
                let x = run.x as usize;
                if x + run.pixels.len() > w {
                    return err(format!("sprite {index}: run past the right edge in row {y}"));
                }
                let at = y * w + x;
                pixels[at..at + run.pixels.len()].copy_from_slice(run.pixels);
                mask[at..at + run.pixels.len()].fill(true);
            }
        }
        Ok(Sprite { width: w as u16, height: h as u16, hot_x: e.hot_x, hot_y: e.hot_y, pixels, mask })
    }
}

/// Font metrics as used by the game's text routines when a sheet is
/// selected with `0x32bb0(sheet, first_char, spacing)`.
///
/// The game always passes `first_char = 0x20`; the spacing is `-5` for
/// `COOLFNT1` in the islands and `0` for `COOLFNT2` in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Font {
    pub first_char: u8,
    pub spacing: i32,
}

impl Font {
    /// Sprite index for a text byte: `byte - first_char`, no translation
    /// table. `None` if the sheet has no such glyph (the game skips the
    /// byte without advancing).
    pub fn glyph(&self, sheet: &SpriteSheet, byte: u8) -> Option<usize> {
        let i = (byte as usize).checked_sub(self.first_char as usize)?;
        // The original accepts `i <= count` (off by one, 0x32c3b); the
        // one-past-the-end index is not a glyph.
        (i < sheet.len()).then_some(i)
    }

    /// Pen advance after a glyph: low byte of its width plus the spacing.
    pub fn advance(&self, sheet: &SpriteSheet, byte: u8) -> i32 {
        match self.glyph(sheet, byte) {
            Some(i) => (sheet.entries()[i].width as u16 & 0xff) as i32 + self.spacing,
            None => 0,
        }
    }

    /// Width of a byte string in pixels (`0x32c10`).
    pub fn text_width(&self, sheet: &SpriteSheet, text: &[u8]) -> i32 {
        text.iter().map(|&b| self.advance(sheet, b)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 4x3 sprite: row 0 has pixels at x=1..=2, row 1 is empty,
    /// row 2 has runs at x=0 and x=3.
    fn sample() -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[4, 0, 3, 0, 0xfe, 0xff, 0xfd, 0xff]);
        data.extend_from_slice(&[1, 0, 10, 0, 1, 2, 2, 0, 7, 8]);
        data.extend_from_slice(&[0, 0, 4, 0]);
        data.extend_from_slice(&[2, 0, 14, 0, 0, 3, 1, 0, 0, 3, 0, 1, 0, 9]);
        let mut v = 1u32.to_le_bytes().to_vec();
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&[4, 0, 3, 0, 0xfe, 0xff, 0xfd, 0xff]);
        v.extend_from_slice(&data);
        v
    }

    #[test]
    fn decodes_sample() {
        let bytes = sample();
        let sheet = SpriteSheet::parse(&bytes).unwrap();
        assert_eq!(sheet.len(), 1);
        let s = sheet.decode(0).unwrap();
        assert!(sheet.trailing_palette().is_none());
        assert_eq!((s.width, s.height, s.hot_x, s.hot_y), (4, 3, -2, -3));
        assert_eq!(s.pixels, [0, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0, 9]);
        let t = true;
        let f = false;
        // The index-0 pixel at (0, 2) is opaque.
        assert_eq!(s.mask, [f, t, t, f, f, f, f, f, t, f, f, t]);
        assert!(sheet.decode(1).is_err());
    }

    #[test]
    fn rejects_truncated() {
        let bytes = sample();
        assert!(SpriteSheet::parse(&bytes[..10]).is_err());
        let sheet = SpriteSheet::parse(&bytes[..bytes.len() - 3]).unwrap();
        assert!(sheet.decode(0).is_err());
    }

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    fn check_sheet(name: &str, bytes: &[u8]) -> usize {
        let sheet = SpriteSheet::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut end = 4 + sheet.len() * ENTRY_SIZE;
        for (i, e) in sheet.entries().iter().enumerate() {
            // Sprites are stored back to back in table order.
            assert_eq!(e.offset as usize + 4, end, "{name} #{i}");
            end += e.size as usize;
            // The data header repeats the table's size and hotspot.
            let d = sheet.sprite_data(i).unwrap();
            let h = |at: usize| i16::from_le_bytes([d[at], d[at + 1]]);
            assert_eq!((h(0), h(2), h(4), h(6)), (e.width, e.height, e.hot_x, e.hot_y), "{name} #{i}");
            for (y, runs) in sheet.runs(i).unwrap().iter().enumerate() {
                let mut x_end = 0usize;
                for r in runs {
                    // Runs are sorted, never overlap, and carry the mirrored column.
                    assert!(r.x as usize >= x_end, "{name} #{i} row {y}");
                    assert_eq!(r.mirror_x as i32, e.width as i32 - 1 - r.x as i32, "{name} #{i} row {y}");
                    x_end = r.x as usize + r.pixels.len();
                }
            }
            let s = sheet.decode(i).unwrap_or_else(|e| panic!("{name} #{i}: {e}"));
            assert_eq!(s.pixels.len(), e.width as usize * e.height as usize);
        }
        // A 768-byte 6-bit palette follows the last sprite.
        assert_eq!(end + 768, bytes.len(), "{name}");
        let pal = sheet.trailing_palette().unwrap_or_else(|| panic!("{name}: no palette"));
        assert!(pal.iter().flatten().all(|&c| c < 64), "{name}");
        sheet.len()
    }

    #[test]
    fn disc_sheets() {
        let Some(bank) = disc("BANQUE.BNK") else {
            eprintln!("skipping: disc files not present");
            return;
        };
        let bnk = crate::bnk::Bnk::parse(&bank).unwrap();
        let mut total = 0;
        for (name, bytes) in bnk.iter() {
            total += check_sheet(name, bytes);
        }
        assert!(total > 2000);
        for name in ["COOLFNT1.SPT", "COOLFNT2.SPT", "COOLFNT3.SPT", "INSTFNT1.SPT", "INSTFNT2.SPT", "INSTFNT3.SPT"] {
            let bytes = disc(name).unwrap();
            check_sheet(name, &bytes);
        }
        // The loose COOLFNT files are copies of the archive members.
        assert_eq!(bnk.get("COOLFNT1.SPT"), disc("COOLFNT1.SPT").as_deref());

        let f1 = disc("COOLFNT1.SPT").unwrap();
        let sheet = SpriteSheet::parse(&f1).unwrap();
        assert_eq!(sheet.len(), 210);
        let font = Font { first_char: 0x20, spacing: -5 };
        assert_eq!(font.glyph(&sheet, b'A'), Some(33));
        assert_eq!(font.glyph(&sheet, 0x1f), None);
        assert_eq!(font.glyph(&sheet, 0xf1), Some(209));
        assert_eq!(font.glyph(&sheet, 0xf2), None);
        assert!(font.text_width(&sheet, b"PUNKY DUCK") > 50);
    }
}
