//! `OBJETS_n.MAP` — texture page for the 3D objects of an island.
//!
//! Spec: `docs/re/formats/map.md`. Follows the second half of the loader at
//! `P.EXE` `0x141a0` (from `0x143fb`): `u32 rows`, `u32 count`, a 768-byte
//! palette that the game reads and immediately overwrites, `count` 12-byte
//! texture records, then `rows * 256` pixels.

use std::fmt;

/// Width of the texture page in pixels (row stride).
pub const PAGE_WIDTH: usize = 256;

const PALETTE_LEN: usize = 0x300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjMapError(pub String);

impl fmt::Display for ObjMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MAP: {}", self.0)
    }
}

impl std::error::Error for ObjMapError {}

/// One texture: a rectangle inside the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texture {
    /// Byte offset of the top-left texel inside the page. This is the only
    /// field the game uses (`0x14f0b`).
    pub offset: u32,
    pub width: u32,
    pub height: u32,
}

impl Texture {
    pub fn x(&self) -> usize {
        self.offset as usize % PAGE_WIDTH
    }

    pub fn y(&self) -> usize {
        self.offset as usize / PAGE_WIDTH
    }
}

/// A parsed `OBJETS_n.MAP`.
#[derive(Debug, Clone)]
pub struct ObjMap {
    /// Page height in pixel rows.
    pub rows: usize,
    /// Embedded 6-bit VGA palette. The game discards it; it equals the
    /// island palette.
    pub palette: [[u8; 3]; 256],
    pub textures: Vec<Texture>,
    /// Page pixels, `rows * PAGE_WIDTH`, row-major.
    pub pixels: Vec<u8>,
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl ObjMap {
    pub fn parse(data: &[u8]) -> Result<Self, ObjMapError> {
        if data.len() < 8 + PALETTE_LEN {
            return Err(ObjMapError("file shorter than the header".into()));
        }
        let rows = u32_at(data, 0) as usize;
        let count = u32_at(data, 4) as usize;
        let mut palette = [[0u8; 3]; 256];
        for (i, c) in palette.iter_mut().enumerate() {
            c.copy_from_slice(&data[8 + i * 3..8 + i * 3 + 3]);
        }
        let table = 8 + PALETTE_LEN;
        let pix = count
            .checked_mul(12)
            .and_then(|n| n.checked_add(table))
            .ok_or_else(|| ObjMapError("texture table size overflows".into()))?;
        let end = rows
            .checked_mul(PAGE_WIDTH)
            .and_then(|n| n.checked_add(pix))
            .ok_or_else(|| ObjMapError("page size overflows".into()))?;
        if data.len() < end {
            return Err(ObjMapError(format!(
                "file has {} bytes, sections need {end}",
                data.len()
            )));
        }
        let textures = (0..count)
            .map(|i| {
                let o = table + i * 12;
                Texture {
                    offset: u32_at(data, o),
                    width: u32_at(data, o + 4),
                    height: u32_at(data, o + 8),
                }
            })
            .collect();
        Ok(ObjMap { rows, palette, textures, pixels: data[pix..end].to_vec() })
    }

    /// Texel `(u, v)` of a texture, addressed the way the rasteriser does:
    /// `pixels[offset + v * 256 + u]`. 0 when outside the page.
    pub fn texel(&self, texture: usize, u: usize, v: usize) -> u8 {
        let Some(t) = self.textures.get(texture) else {
            return 0;
        };
        self.pixels
            .get(t.offset as usize + v * PAGE_WIDTH + u)
            .copied()
            .unwrap_or(0)
    }

    /// Copy of one texture rectangle, `width * height` bytes, row-major.
    pub fn texture_pixels(&self, texture: usize) -> Option<Vec<u8>> {
        let t = self.textures.get(texture)?;
        let (w, h) = (t.width as usize, t.height as usize);
        let mut out = Vec::with_capacity(w * h);
        for v in 0..h {
            for u in 0..w {
                out.push(self.texel(texture, u, v));
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    #[test]
    fn synthetic() {
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&[0u8; 768]);
        for v in [257u32, 3, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let mut pix = vec![0u8; 512];
        pix[257] = 9;
        pix[259] = 7;
        d.extend_from_slice(&pix);
        let m = ObjMap::parse(&d).unwrap();
        assert_eq!((m.textures[0].x(), m.textures[0].y()), (1, 1));
        assert_eq!(m.texture_pixels(0).unwrap(), vec![9, 0, 7]);
        assert!(ObjMap::parse(&d[..d.len() - 1]).is_err());
    }

    #[test]
    fn disc_pages() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("OBJETS_{n}.MAP")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            let m = ObjMap::parse(&d).unwrap();
            // No trailing bytes: the layout accounts for the whole file.
            assert_eq!(d.len(), 8 + 768 + m.textures.len() * 12 + m.rows * PAGE_WIDTH);
            for t in &m.textures {
                assert!(t.x() + t.width as usize <= PAGE_WIDTH);
                assert!(t.y() + t.height as usize <= m.rows);
            }
        }
    }
}
