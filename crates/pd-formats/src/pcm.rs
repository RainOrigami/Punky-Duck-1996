//! `.PCM` sound effects: headerless signed 16-bit little-endian mono
//! samples, played at 16000 Hz.
//!
//! Spec: `docs/re/formats/pcm.md`. Follows the sample loader
//! (`P.EXE` `0x2ef80`, `0x2f020`) and the mixers (`0x2e4d0` and siblings).

use std::fmt;

/// Output rate the game requests from the DIGPAK driver in every mode
/// (`mov word [snd+0xa], 0x3e80` at `0x2f37e`, `0x2f5b4`, `0x2f659`, ...).
/// The mixers step one source sample per output frame, so this is also the
/// sample rate of the files.
pub const SAMPLE_RATE: u32 = 16000;

/// The mixers read one `i16` per output frame from each voice.
pub const CHANNELS: u16 = 1;

/// Number of simultaneous voices in the game's mixer.
pub const MIXER_VOICES: usize = 8;

/// Samples mixed per driver buffer ("Swid: %d" at `0x2f2eb`).
pub const MIX_BLOCK: usize = 630;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmError(pub String);

impl fmt::Display for PcmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PCM: {}", self.0)
    }
}

impl std::error::Error for PcmError {}

/// A decoded sound effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm {
    pub samples: Vec<i16>,
}

impl Pcm {
    /// Decodes a `.PCM` file. A trailing odd byte is an error (none of the
    /// shipped files has one).
    pub fn parse(data: &[u8]) -> Result<Self, PcmError> {
        if data.len() % 2 != 0 {
            return Err(PcmError(format!("odd length {}", data.len())));
        }
        let samples = data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        Ok(Self { samples })
    }

    pub fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }

    /// `true` for the placeholder files (`PAS-G-11.PCM`, `PAS-D-11.PCM`:
    /// five zero samples), which play as nothing.
    pub fn is_silent(&self) -> bool {
        self.samples.iter().all(|&s| s == 0)
    }

    /// Samples converted to `f32` in `-1.0..1.0`.
    pub fn to_f32(&self) -> Vec<f32> {
        self.samples.iter().map(|&s| s as f32 / 32768.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_samples() {
        let pcm = Pcm::parse(&[0x00, 0x00, 0xff, 0x7f, 0x00, 0x80, 0xec, 0xff]).unwrap();
        assert_eq!(pcm.samples, [0, 32767, -32768, -20]);
        assert!(!pcm.is_silent());
        assert!(Pcm::parse(&[1, 2, 3]).is_err());
        assert!(Pcm::parse(&[]).unwrap().is_silent());
    }

    #[test]
    fn disc_samples() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/SAMPLE.BNK");
        let Ok(bank) = std::fs::read(p) else {
            eprintln!("skipping: disc files not present");
            return;
        };
        let bnk = crate::bnk::Bnk::parse(&bank).unwrap();
        let mut stubs = Vec::new();
        for (name, bytes) in bnk.iter() {
            let pcm = Pcm::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            if pcm.is_silent() {
                assert_eq!(pcm.samples.len(), 5, "{name}");
                stubs.push(name.to_string());
                continue;
            }
            // Real effects last between a fraction of a second and a few
            // seconds at 16 kHz, and as 16-bit signed data they start and
            // stay near zero on average (no DC offset as unsigned data would show).
            let secs = pcm.duration_secs();
            assert!((0.04..7.0).contains(&secs), "{name}: {secs}s");
            let mean = pcm.samples.iter().map(|&s| s as f64).sum::<f64>() / pcm.samples.len() as f64;
            assert!(mean.abs() < 1500.0, "{name}: mean {mean}");
        }
        assert_eq!(stubs, ["PAS-D-11.PCM", "PAS-G-11.PCM"]);
    }
}
