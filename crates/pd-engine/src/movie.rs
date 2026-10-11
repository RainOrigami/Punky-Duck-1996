//! FLC movie playback as an [`App`]: no loops, no clock; time arrives through
//! `update`.

use pd_formats::flc::{Flc, FlcError};

use crate::{App, Command, Framebuffer, Input, SCREEN_H, SCREEN_W};

/// Plays one FLC movie once. Frame pacing follows the delay in the FLC header.
/// Skippable by a fresh press of action (Ctrl), secondary (Space), item
/// (Enter), cancel (Esc) or the left mouse button.
pub struct MoviePlayer {
    flc: Flc<Vec<u8>>,
    /// Microseconds until the next frame is due (<= 0: due now).
    until_next_us: i64,
    skippable: bool,
    /// Skip-button state seen on the previous update; `None` before the first
    /// update, so a button still held from an earlier screen does not skip.
    prev_skip: Option<bool>,
    finished: bool,
    skipped: bool,
    error: Option<FlcError>,
}

impl MoviePlayer {
    pub fn new(data: Vec<u8>) -> Result<Self, FlcError> {
        let flc = Flc::new(data)?;
        if flc.width() > SCREEN_W || flc.height() > SCREEN_H {
            return Err(FlcError(format!(
                "movie is {}x{}, larger than the {SCREEN_W}x{SCREEN_H} screen",
                flc.width(),
                flc.height()
            )));
        }
        Ok(Self {
            flc,
            until_next_us: 0,
            skippable: true,
            prev_skip: None,
            finished: false,
            skipped: false,
            error: None,
        })
    }

    pub fn set_skippable(&mut self, skippable: bool) {
        self.skippable = skippable;
    }

    /// True once the last frame's delay elapsed, the movie was skipped, or
    /// decoding failed (see [`error`](Self::error)).
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn was_skipped(&self) -> bool {
        self.skipped
    }

    /// Decode error that ended playback early, if any.
    pub fn error(&self) -> Option<&FlcError> {
        self.error.as_ref()
    }

    /// Number of frames shown so far.
    pub fn frames_shown(&self) -> u32 {
        self.flc.frames_decoded()
    }

    pub fn total_frames(&self) -> u32 {
        self.flc.header().frames as u32
    }
}

/// Colour component as the VGA shows it. The FLC palette has 8 bits per
/// channel but the DAC of the original's video mode has 6, so the low two
/// bits are lost; the 6-bit value is then widened by bit replication.
/// Verified: with this mapping decoded frames are pixel-identical to a
/// lossless DOSBox-X capture of `INTRO.EXE` (see `docs/re/formats/flc.md`).
pub fn vga_dac(v: u8) -> u8 {
    let c6 = v >> 2;
    (c6 << 2) | (c6 >> 4)
}

impl App for MoviePlayer {
    fn update(&mut self, dt_us: u64, input: &Input, _commands: &mut Vec<Command>) {
        if self.finished {
            return;
        }
        let skip = input.action || input.secondary || input.item || input.cancel || input.mouse_left;
        let pressed = self.prev_skip == Some(false) && skip;
        self.prev_skip = Some(skip);
        if self.skippable && pressed {
            self.finished = true;
            self.skipped = true;
            return;
        }

        self.until_next_us -= dt_us.min(i64::MAX as u64) as i64;
        while self.until_next_us <= 0 {
            match self.flc.decode_next() {
                Ok(Some(info)) => self.until_next_us += info.delay_ms.max(1) as i64 * 1000,
                Ok(None) => {
                    self.finished = true;
                    break;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.finished = true;
                    break;
                }
            }
        }
    }

    fn render(&mut self, fb: &mut Framebuffer) {
        for (dst, src) in fb.palette.iter_mut().zip(self.flc.palette()) {
            *dst = src.map(vga_dac);
        }
        let (w, h) = (self.flc.width(), self.flc.height());
        if w == SCREEN_W && h == SCREEN_H {
            fb.pixels.copy_from_slice(self.flc.pixels());
            return;
        }
        // Smaller movies are centred on colour 0.
        fb.pixels.fill(0);
        let (x0, y0) = ((SCREEN_W - w) / 2, (SCREEN_H - h) / 2);
        for (y, row) in self.flc.pixels().chunks_exact(w).enumerate() {
            let at = (y0 + y) * SCREEN_W + x0;
            fb.pixels[at..at + w].copy_from_slice(row);
        }
    }

    fn mix_audio(&mut self, out: &mut [i16]) {
        // The movies have no sound track of their own; music comes from the CD.
        out.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dac_mapping() {
        assert_eq!(vga_dac(0), 0);
        assert_eq!(vga_dac(3), 0);
        assert_eq!(vga_dac(4), 4);
        assert_eq!(vga_dac(0x80), 0x82);
        assert_eq!(vga_dac(255), 255);
        assert_eq!(vga_dac(252), 255);
    }
}
