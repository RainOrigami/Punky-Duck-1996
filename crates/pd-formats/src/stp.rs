//! `GAME.STP` — saved settings, 8 little-endian `u32`.
//!
//! Spec: `docs/re/formats/stp.md`. Read at `P.EXE` `0x258c4`, written at
//! `0x25a40`; field meanings from the option menus that edit the variables
//! (`0x26650` "OPTIONS", `0x26980` "SOUND AND MUSIC", `0x26bc0` "CONTROLLER").

use std::fmt;

/// File size.
pub const FILE_SIZE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StpError(pub String);

impl fmt::Display for StpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "STP: {}", self.0)
    }
}

impl std::error::Error for StpError {}

/// The settings, in file order. Values are kept raw; the game does not
/// range-check them on load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stp {
    /// 0 = "SOUND OFF", 1 = "SOUND ON" (`0x41610`).
    pub sound_on: u32,
    /// 0 = "MUSIC OFF", 1 = "MUSIC ON" (`0x41614`).
    pub music_on: u32,
    /// 0 = "KEYBOARD", 1 = "JOYSTICK" (`0x41618`).
    pub controller: u32,
    /// "SFX VOLUME", 0..=15 (`0x4161c`).
    pub sfx_volume: u32,
    /// "CD VOLUME", 0..=15 (`0x41620`).
    pub cd_volume: u32,
    /// 0 = "EASY !", 1 = "MEDIUM !!", 2 = "HARD !!!" (`0x41624`).
    pub difficulty: u32,
    /// 0 = French, 1 = English, 2 = Spanish, 3 = German, 4 = Polish (`0x416e4`).
    pub language: u32,
    /// "GAME SPEED", -6..=6 (`0x4000c`).
    pub game_speed: i32,
}

/// Menu names of [`Stp::language`], in index order (`0x417d4`).
pub const LANGUAGES: [&str; 5] = ["FRENCH", "ENGLISH", "SPANISH", "GERMAN", "POLISH"];
/// Menu names of [`Stp::difficulty`], in index order (`0x417ec`).
pub const DIFFICULTIES: [&str; 3] = ["EASY !", "MEDIUM !!", "HARD !!!"];

impl Stp {
    pub fn parse(data: &[u8]) -> Result<Self, StpError> {
        if data.len() < FILE_SIZE {
            return Err(StpError(format!(
                "file has {} bytes, need {FILE_SIZE}",
                data.len()
            )));
        }
        let w = |i: usize| u32::from_le_bytes([data[i * 4], data[i * 4 + 1], data[i * 4 + 2], data[i * 4 + 3]]);
        Ok(Stp {
            sound_on: w(0),
            music_on: w(1),
            controller: w(2),
            sfx_volume: w(3),
            cd_volume: w(4),
            difficulty: w(5),
            language: w(6),
            game_speed: w(7) as i32,
        })
    }

    /// The 32 bytes the game writes at `0x25a40`.
    pub fn to_bytes(&self) -> [u8; FILE_SIZE] {
        let words = [
            self.sound_on,
            self.music_on,
            self.controller,
            self.sfx_volume,
            self.cd_volume,
            self.difficulty,
            self.language,
            self.game_speed as u32,
        ];
        let mut out = [0u8; FILE_SIZE];
        for (i, w) in words.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Stp {
            sound_on: 1,
            music_on: 0,
            controller: 1,
            sfx_volume: 15,
            cd_volume: 5,
            difficulty: 2,
            language: 4,
            game_speed: -6,
        };
        assert_eq!(Stp::parse(&s.to_bytes()).unwrap(), s);
        assert!(Stp::parse(&[0u8; 31]).is_err());
    }

    #[test]
    fn disc_file() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/GAME.STP");
        let Ok(d) = std::fs::read(p) else {
            eprintln!("disc files absent, skipping");
            return;
        };
        assert_eq!(d.len(), FILE_SIZE);
        let s = Stp::parse(&d).unwrap();
        assert_eq!(s.to_bytes().as_slice(), d.as_slice());
        assert!(s.sfx_volume <= 15 && s.cd_volume <= 15);
        assert!((s.language as usize) < LANGUAGES.len());
        assert!((s.difficulty as usize) < DIFFICULTIES.len());
    }
}
