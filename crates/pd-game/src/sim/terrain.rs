//! Terrain queries over the decoded `ILE_n.UNI`: height sampler (`0x13010`), zone sampler
//! (`0x151c0`), zone to monster-flag table (`0x415d4`) and `zone_fill` (`0x28ce0`).
//!
//! Both maps are indexed `x * 256 + z` like the original's `[x][z]` arrays. The zone map
//! is patched at run time by the island scripts, so it lives here, not in the UNI.

use pd_formats::uni::Uni;

/// World units per terrain cell (`0x4f0ec0`).
pub const CELL: i32 = 300;
const SIDE: usize = 256;

/// Monster flags per zone type (`0x415d4`, 32 bytes). Bit 1: flyers blocked, bit 2:
/// walkers blocked, bit 4: walkers at half speed, bit 8: stops monster shots,
/// bit 0x10: swimmers allowed.
pub const ZONE_FLAGS: [u8; 32] = [
    0, 18, 20, 11, 4, 0, 11, 0, 11, 4, 11, 11, 0, 0, 11, 11, 0, 11, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// Zone (attribute) map `0x22cdc8`, `[x][z]`.
    pub zone: Vec<u8>,
    /// Height map `0x1f2668` (i16, file byte x 3), `[x][z]`.
    pub height: Vec<i16>,
}

impl Default for Terrain {
    fn default() -> Self {
        Terrain { zone: vec![0; SIDE * SIDE], height: vec![0; SIDE * SIDE] }
    }
}

impl Terrain {
    pub fn from_uni(uni: &Uni) -> Self {
        Terrain {
            zone: uni.attrs.clone(),
            // The loader converts each byte with `* 3.0` (`0x1283a`).
            height: uni.heights.iter().map(|&b| b as i16 * 3).collect(),
        }
    }

    fn h(&self, flat: i32) -> i32 {
        // The original has no bounds check and reads neighbouring memory; out of the
        // array this returns 0.
        if (0..(SIDE * SIDE) as i32).contains(&flat) {
            self.height[flat as usize] as i32
        } else {
            0
        }
    }

    /// `terrain_height(x, z)` (`0x13010`): bilinear sample, C integer arithmetic.
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        let x = x.wrapping_add(CELL * 128);
        let z = z.wrapping_add(CELL * 128);
        let xf = x % CELL;
        let xc = x / CELL;
        let zc = z / CELL;
        let zf = z % CELL;
        let i0 = xc * 256 + zc;
        let i1 = (xc + 1) * 256 + zc;
        let a = self.h(i0) + (self.h(i1) - self.h(i0)) * xf / CELL;
        let b = (self.h(i1 + 1) - self.h(i0 + 1)) * xf / CELL + self.h(i0 + 1);
        ((b - a) * zf / CELL + a) * CELL
    }

    /// `zone_at(x, z)` (`0x151c0`): cell index by truncating division, clamped to
    /// -126..=126, zone byte at `[xc + 128][zc + 128]`.
    pub fn zone_at(&self, x: i32, z: i32) -> i32 {
        let xc = (x / CELL).clamp(-126, 126);
        let zc = (z / CELL).clamp(-126, 126);
        self.zone[((xc + 128) * 256 + zc + 128) as usize] as i32
    }

    /// Monster flags of the zone at a world position (`0x21880`).
    pub fn flags_at(&self, x: i32, z: i32) -> u8 {
        ZONE_FLAGS[(self.zone_at(x, z) & 31) as usize]
    }

    /// `zone_fill(x0, z0, x1, z1, v)` (`0x28ce0`). As coded it writes `Z[x][z + 1]`
    /// (base address `0x22cdc9`), one cell further along Z than its arguments say.
    pub fn zone_fill(&mut self, x0: i32, z0: i32, x1: i32, z1: i32, v: u8) {
        for z in z0..=z1 {
            for x in x0..=x1 {
                let i = x * 256 + z + 1;
                if (0..(SIDE * SIDE) as i32).contains(&i) {
                    self.zone[i as usize] = v;
                }
            }
        }
    }

    /// Zone byte by flat cell index, 0 outside the map (used by the random money
    /// placer, which indexes neighbours without bounds checks).
    pub fn zone_flat(&self, flat: i32) -> u8 {
        if (0..(SIDE * SIDE) as i32).contains(&flat) {
            self.zone[flat as usize]
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_is_bilinear_times_cell() {
        let mut t = Terrain::default();
        // cell (128,128) corner heights 30, 60 along x; 90, 120 along z + 1
        t.height[128 * 256 + 128] = 30;
        t.height[129 * 256 + 128] = 60;
        t.height[128 * 256 + 129] = 90;
        t.height[129 * 256 + 129] = 120;
        assert_eq!(t.height_at(0, 0), 30 * 300);
        assert_eq!(t.height_at(150, 0), 45 * 300);
        assert_eq!(t.height_at(0, 150), 60 * 300);
        assert_eq!(t.height_at(150, 150), 75 * 300);
        // integer truncation: (60-30)*100/300 = 10
        assert_eq!(t.height_at(100, 0), 40 * 300);
    }

    #[test]
    fn zone_lookup_truncates_and_clamps() {
        let mut t = Terrain::default();
        t.zone[128 * 256 + 128] = 7;
        t.zone[(128 + 126) * 256 + 128] = 9;
        // -299..=299 all truncate to cell 0
        assert_eq!(t.zone_at(-299, 299), 7);
        assert_eq!(t.zone_at(300, 0), 0);
        assert_eq!(t.zone_at(1_000_000, 0), 9);
    }

    #[test]
    fn zone_fill_is_off_by_one_in_z() {
        let mut t = Terrain::default();
        t.zone_fill(10, 20, 11, 20, 17);
        assert_eq!(t.zone[10 * 256 + 20], 0);
        assert_eq!(t.zone[10 * 256 + 21], 17);
        assert_eq!(t.zone[11 * 256 + 21], 17);
    }

    #[test]
    fn flags_table() {
        assert_eq!(ZONE_FLAGS[1], 0x12);
        assert_eq!(ZONE_FLAGS[2], 0x14);
        assert_eq!(ZONE_FLAGS[8], 0x0b);
        assert_eq!(ZONE_FLAGS[18], 0x0b);
    }
}
