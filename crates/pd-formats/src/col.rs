//! `ILE_n.COL` — in-game palette of an island.
//!
//! Spec: `docs/re/formats/col.md`. Follows the loader at `P.EXE` `0x33440`:
//! 8 header bytes are read and discarded, then 768 bytes of 8-bit RGB are
//! read and each component is divided by 4 to get the 6-bit VGA DAC value.
//! The header (`u32 file size = 776`, `u16 0xB123`, `u16 0`) is the Autodesk
//! Animator Pro `.COL` header.

use std::fmt;

/// Total file size.
pub const FILE_SIZE: usize = 776;
/// Size of the ignored header.
pub const HEADER_SIZE: usize = 8;
/// Magic word at offset 4.
pub const MAGIC: u16 = 0xB123;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColError(pub String);

impl fmt::Display for ColError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "COL: {}", self.0)
    }
}

impl std::error::Error for ColError {}

/// A parsed `.COL` palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Col {
    /// Header field at offset 0 (776 in every disc file). Not checked by the game.
    pub size_field: u32,
    /// Header field at offset 4 (`0xB123`). Not checked by the game.
    pub magic: u16,
    /// Header field at offset 6 (0). Not checked by the game.
    pub version: u16,
    /// 8-bit RGB as stored.
    pub rgb8: [[u8; 3]; 256],
}

impl Col {
    pub fn parse(data: &[u8]) -> Result<Self, ColError> {
        if data.len() < FILE_SIZE {
            return Err(ColError(format!(
                "file has {} bytes, need {FILE_SIZE}",
                data.len()
            )));
        }
        let mut rgb8 = [[0u8; 3]; 256];
        for (i, c) in rgb8.iter_mut().enumerate() {
            let o = HEADER_SIZE + i * 3;
            c.copy_from_slice(&data[o..o + 3]);
        }
        Ok(Col {
            size_field: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            magic: u16::from_le_bytes([data[4], data[5]]),
            version: u16::from_le_bytes([data[6], data[7]]),
            rgb8,
        })
    }

    /// The palette as the game holds it after loading: 6-bit VGA values
    /// (`value / 4`, `0x33490`).
    pub fn vga6(&self) -> [[u8; 3]; 256] {
        let mut out = self.rgb8;
        for c in out.iter_mut() {
            for v in c.iter_mut() {
                *v >>= 2;
            }
        }
        out
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
        let mut d = vec![0u8; FILE_SIZE];
        d[0..4].copy_from_slice(&776u32.to_le_bytes());
        d[4..6].copy_from_slice(&MAGIC.to_le_bytes());
        d[8 + 3] = 255;
        d[8 + 4] = 7;
        let c = Col::parse(&d).unwrap();
        assert_eq!(c.rgb8[1], [255, 7, 0]);
        assert_eq!(c.vga6()[1], [63, 1, 0]);
        assert!(Col::parse(&d[..775]).is_err());
    }

    #[test]
    fn disc_palettes() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("ILE_{n}.COL")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            assert_eq!(d.len(), FILE_SIZE);
            let c = Col::parse(&d).unwrap();
            assert_eq!(c.size_field, 776);
            assert_eq!(c.magic, MAGIC);
            assert_eq!(c.version, 0);
        }
    }
}
