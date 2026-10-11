//! `ILE_n.UNI` — island terrain ("univers").
//!
//! Spec: `docs/re/formats/uni.md`. Follows the loader at `P.EXE` `0x12700`.
//! The file is not an image and is not compressed: it is a 256x256 cell
//! height-mapped terrain whose cells are textured with 32x32 tiles cut from a
//! 256 pixel wide tile sheet.
//!
//! All per-cell arrays are indexed `x * 256 + z` (first index = world X,
//! second = world Z), exactly as the game indexes them (`0x129df`, `0x13071`).

use std::fmt;

/// Cells per side of the terrain grid.
pub const GRID: usize = 256;
/// Tile edge length in pixels.
pub const TILE: usize = 32;
/// Width of the tile sheet in pixels (row stride of tile data).
pub const SHEET_WIDTH: usize = 256;
/// World units per cell (`0x126e4 * 0x126e8` = 20 * 15, stored at `0x4f0ec0`).
pub const CELL_SIZE: i32 = 300;
/// World units per height-map step (`0x423ef` = 3.0).
pub const HEIGHT_SCALE: i32 = 3;
/// Number of shade levels in the shade table block.
pub const SHADE_LEVELS: usize = 32;

const PALETTE_LEN: usize = 0x300;
const SHADE_LEN: usize = SHADE_LEVELS * 256;
const CELLS: usize = GRID * GRID;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniError(pub String);

impl fmt::Display for UniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UNI: {}", self.0)
    }
}

impl std::error::Error for UniError {}

/// Decoded fields of one `u16` terrain cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Tile number, index into [`Uni::tile_offsets`] (`cell & 0xfff`).
    pub tile: u16,
    /// Texture rotation in quarter turns (`(cell >> 13) & 3`).
    pub rotation: u8,
}

impl Cell {
    pub fn from_raw(raw: u16) -> Self {
        Cell { tile: raw & 0x0fff, rotation: ((raw >> 13) & 3) as u8 }
    }

    /// Tile texel `(u, v)` shown at in-cell position `(dx, dz)`, each
    /// `0..TILE`, `dx` growing with world X and `dz` with world Z.
    /// Derived from the per-rotation corner assignments at `0x12dc3`.
    pub fn texel(self, dx: usize, dz: usize) -> (usize, usize) {
        let m = TILE - 1;
        match self.rotation {
            0 => (dx, dz),
            1 => (dz, m - dx),
            2 => (m - dx, m - dz),
            _ => (m - dz, dx),
        }
    }
}

/// A parsed `ILE_n.UNI`.
#[derive(Debug, Clone)]
pub struct Uni {
    /// 6-bit VGA palette (values `0..=63`). The game replaces it with
    /// `ILE_n.COL` before the island is shown.
    pub palette: [[u8; 3]; 256],
    /// `SHADE_LEVELS` colour remap tables of 256 bytes, index
    /// `level * 256 + colour`.
    pub shade_tables: Vec<u8>,
    /// Tile sheet height in pixel rows.
    pub sheet_rows: usize,
    /// Tile sheet pixels, `sheet_rows * SHEET_WIDTH`, row-major.
    pub sheet: Vec<u8>,
    /// Byte offset of each tile's top-left texel inside [`Uni::sheet`].
    pub tile_offsets: Vec<u32>,
    /// Raw terrain cells, index `x * 256 + z`. See [`Cell`].
    pub cells: Vec<u16>,
    /// Per-cell attribute byte, index `x * 256 + z`.
    pub attrs: Vec<u8>,
    /// Per-cell height, index `x * 256 + z`; world height is
    /// `value * HEIGHT_SCALE`.
    pub heights: Vec<u8>,
    /// Four bytes per tile that follow the height map. The game never reads
    /// them (the file is closed after the height map).
    pub tile_extra: Vec<[u8; 4]>,
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl Uni {
    pub fn parse(data: &[u8]) -> Result<Self, UniError> {
        let head = PALETTE_LEN + SHADE_LEN + 8;
        if data.len() < head {
            return Err(UniError("file shorter than the fixed header".into()));
        }
        let mut palette = [[0u8; 3]; 256];
        for (i, c) in palette.iter_mut().enumerate() {
            c.copy_from_slice(&data[i * 3..i * 3 + 3]);
        }
        let shade_tables = data[PALETTE_LEN..PALETTE_LEN + SHADE_LEN].to_vec();
        let sheet_rows = u32_at(data, PALETTE_LEN + SHADE_LEN) as usize;
        let tile_count = u32_at(data, PALETTE_LEN + SHADE_LEN + 4) as usize;

        let sheet_len = sheet_rows
            .checked_mul(SHEET_WIDTH)
            .ok_or_else(|| UniError("tile sheet size overflows".into()))?;
        let table_len = tile_count
            .checked_mul(4)
            .ok_or_else(|| UniError("tile table size overflows".into()))?;
        let need = head
            .checked_add(sheet_len)
            .and_then(|n| n.checked_add(table_len))
            .and_then(|n| n.checked_add(CELLS * 4))
            .ok_or_else(|| UniError("section sizes overflow".into()))?;
        if data.len() < need {
            return Err(UniError(format!(
                "file has {} bytes, sections need {need}",
                data.len()
            )));
        }

        let mut o = head;
        let sheet = data[o..o + sheet_len].to_vec();
        o += sheet_len;
        let tile_offsets: Vec<u32> = (0..tile_count).map(|i| u32_at(data, o + i * 4)).collect();
        o += table_len;
        let cells: Vec<u16> = (0..CELLS)
            .map(|i| u16::from_le_bytes([data[o + i * 2], data[o + i * 2 + 1]]))
            .collect();
        o += CELLS * 2;
        let attrs = data[o..o + CELLS].to_vec();
        o += CELLS;
        let heights = data[o..o + CELLS].to_vec();
        o += CELLS;
        let tile_extra = data[o..]
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();

        Ok(Uni {
            palette,
            shade_tables,
            sheet_rows,
            sheet,
            tile_offsets,
            cells,
            attrs,
            heights,
            tile_extra,
        })
    }

    /// Palette expanded from 6-bit VGA to 8-bit RGB.
    pub fn palette_rgb8(&self) -> [[u8; 3]; 256] {
        let mut out = [[0u8; 3]; 256];
        for (o, c) in out.iter_mut().zip(self.palette.iter()) {
            for k in 0..3 {
                let v = c[k] & 0x3f;
                o[k] = (v << 2) | (v >> 4);
            }
        }
        out
    }

    pub fn cell(&self, x: usize, z: usize) -> Cell {
        Cell::from_raw(self.cells[x * GRID + z])
    }

    pub fn attr(&self, x: usize, z: usize) -> u8 {
        self.attrs[x * GRID + z]
    }

    pub fn height(&self, x: usize, z: usize) -> u8 {
        self.heights[x * GRID + z]
    }

    /// One texel of a tile; 0 when the tile or texel lies outside the sheet.
    pub fn tile_texel(&self, tile: u16, u: usize, v: usize) -> u8 {
        let Some(&off) = self.tile_offsets.get(tile as usize) else {
            return 0;
        };
        self.sheet
            .get(off as usize + v * SHEET_WIDTH + u)
            .copied()
            .unwrap_or(0)
    }

    /// Top-down picture of the whole terrain, `px` pixels per cell
    /// (`px` must divide [`TILE`]; 32 gives the full 8192x8192 picture).
    /// Returns `(side, pixels)`; image column = cell X, row = cell Z, the
    /// orientation the game's own map screen uses (`0x28259`).
    pub fn render(&self, px: usize) -> (usize, Vec<u8>) {
        assert!(px > 0 && TILE % px == 0, "px must divide the tile size");
        let step = TILE / px;
        let side = GRID * px;
        let mut out = vec![0u8; side * side];
        for x in 0..GRID {
            for z in 0..GRID {
                let cell = self.cell(x, z);
                for j in 0..px {
                    for i in 0..px {
                        let (u, v) = cell.texel(i * step, j * step);
                        out[(z * px + j) * side + x * px + i] = self.tile_texel(cell.tile, u, v);
                    }
                }
            }
        }
        (side, out)
    }
}

/// Cell coordinate of a world coordinate (`0x13025`: `(w + 128*300) / 300`).
pub fn world_to_cell(w: i32) -> i32 {
    (w + (GRID as i32 / 2) * CELL_SIZE).div_euclid(CELL_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    #[test]
    fn rotations_map_corners() {
        // rot 1, corner (x, z): u = dz, v = 31 - dx (0x12e17).
        let c = Cell::from_raw(1 << 13);
        assert_eq!(c.texel(0, 0), (0, 31));
        assert_eq!(c.texel(31, 0), (0, 0));
        assert_eq!(Cell::from_raw(3 << 13 | 5).tile, 5);
        assert_eq!(Cell::from_raw(3 << 13).texel(0, 0), (31, 0));
        assert_eq!(Cell::from_raw(2 << 13).texel(0, 0), (31, 31));
    }

    #[test]
    fn rejects_short() {
        assert!(Uni::parse(&[0u8; 100]).is_err());
    }

    #[test]
    fn disc_islands() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("ILE_{n}.UNI")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            let u = Uni::parse(&d).unwrap();
            assert_eq!(u.sheet_rows % TILE, 0);
            // Every referenced tile exists and lies wholly inside the sheet.
            for &raw in &u.cells {
                assert_eq!(raw & 0x9000, 0, "bits 12 and 15 are never set");
                let c = Cell::from_raw(raw);
                let off = u.tile_offsets[c.tile as usize] as usize;
                assert_eq!(off % TILE, 0);
                assert!(off + (TILE - 1) * SHEET_WIDTH + TILE <= u.sheet.len());
            }
            // The unread trailer is exactly 4 bytes per tile.
            assert_eq!(u.tile_extra.len(), u.tile_offsets.len());
            assert!(u.palette.iter().flatten().all(|&v| v < 64));
            let (side, img) = u.render(1);
            assert_eq!(side, GRID);
            assert_eq!(img.len(), GRID * GRID);
        }
    }
}
