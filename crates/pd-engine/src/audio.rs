//! Audio building blocks: a sound-effect mixer for [`App`](crate::App)
//! implementations and the CD-music player used by the frontends.
//!
//! Everything produces interleaved stereo `i16` at [`SAMPLE_RATE`].
//!
//! - [`SfxMixer`]: the mixer of `P.EXE` (8 channels, 16 kHz mono, original
//!   volume law and channel allocation). Game logic uses this one.
//! - [`Mixer`]: generic voices at any source rate with volume and pan, for
//!   anything the original mixer cannot express (and the demo's test tone).
//! - [`CdPlayer`]: Red Book music from the disc image.

use std::rc::Rc;
use std::sync::Arc;

use pd_formats::disc::{AUDIO_RATE, Disc};

use crate::{CHANNELS, SAMPLE_RATE};

/// Unity gain for [`VoiceParams::volume`] and the master volumes.
pub const VOLUME_UNITY: u16 = 256;
/// [`VoiceParams::pan`] range is `-PAN_RANGE..=PAN_RANGE` (left..right).
pub const PAN_RANGE: i16 = 256;

/// Immutable PCM sample data, cheap to clone and share between voices.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Interleaved if `channels == 2`.
    pub data: Arc<[i16]>,
    pub rate: u32,
    /// 1 or 2.
    pub channels: u8,
}

impl Sample {
    pub fn mono(data: impl Into<Arc<[i16]>>, rate: u32) -> Self {
        Self { data: data.into(), rate, channels: 1 }
    }

    pub fn stereo(data: impl Into<Arc<[i16]>>, rate: u32) -> Self {
        Self { data: data.into(), rate, channels: 2 }
    }

    /// Length in sample frames.
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceParams {
    /// `0..=VOLUME_UNITY` (larger values amplify).
    pub volume: u16,
    /// `-PAN_RANGE` = hard left, `0` = centre, `PAN_RANGE` = hard right.
    pub pan: i16,
    pub looping: bool,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self { volume: VOLUME_UNITY, pan: 0, looping: false }
    }
}

struct Voice {
    sample: Sample,
    params: VoiceParams,
    /// Position in source frames, 32.32 fixed point.
    pos: u64,
    /// Source frames per output frame, 32.32 fixed point.
    step: u64,
}

impl Voice {
    /// Source frame `i` as `[left, right]`, honouring looping for the
    /// interpolation neighbour. `None` past the end.
    fn frame(&self, i: usize) -> Option<[i32; 2]> {
        let n = self.sample.frames();
        let i = if i < n {
            i
        } else if self.params.looping && n > 0 {
            i % n
        } else {
            return None;
        };
        Some(if self.sample.channels == 2 {
            [self.sample.data[i * 2] as i32, self.sample.data[i * 2 + 1] as i32]
        } else {
            let v = self.sample.data[i] as i32;
            [v, v]
        })
    }
}

/// Fixed number of voices playing `i16` PCM at arbitrary source rates,
/// linearly resampled to [`SAMPLE_RATE`], with per-voice volume and pan.
pub struct Mixer {
    voices: Vec<Option<Voice>>,
    master: u16,
}

impl Mixer {
    pub fn new(voices: usize) -> Self {
        Self { voices: (0..voices).map(|_| None).collect(), master: VOLUME_UNITY }
    }

    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    pub fn set_master_volume(&mut self, volume: u16) {
        self.master = volume;
    }

    /// Start `sample` on the first free voice; `None` if all voices are busy.
    pub fn play(&mut self, sample: &Sample, params: VoiceParams) -> Option<usize> {
        let idx = self.voices.iter().position(Option::is_none)?;
        self.play_on(idx, sample, params);
        Some(idx)
    }

    /// Start `sample` on voice `idx`, replacing whatever it was playing.
    pub fn play_on(&mut self, idx: usize, sample: &Sample, params: VoiceParams) {
        if idx >= self.voices.len() || sample.frames() == 0 || sample.rate == 0 {
            return;
        }
        let step = ((sample.rate as u64) << 32) / SAMPLE_RATE as u64;
        self.voices[idx] = Some(Voice { sample: sample.clone(), params, pos: 0, step });
    }

    pub fn stop(&mut self, idx: usize) {
        if let Some(v) = self.voices.get_mut(idx) {
            *v = None;
        }
    }

    pub fn stop_all(&mut self) {
        self.voices.iter_mut().for_each(|v| *v = None);
    }

    pub fn is_playing(&self, idx: usize) -> bool {
        self.voices.get(idx).is_some_and(Option::is_some)
    }

    pub fn set_volume(&mut self, idx: usize, volume: u16) {
        if let Some(Some(v)) = self.voices.get_mut(idx) {
            v.params.volume = volume;
        }
    }

    pub fn set_pan(&mut self, idx: usize, pan: i16) {
        if let Some(Some(v)) = self.voices.get_mut(idx) {
            v.params.pan = pan;
        }
    }

    /// Overwrite `out` (interleaved stereo) with the mix of all voices.
    /// This is what an `App::mix_audio` implementation calls.
    pub fn fill(&mut self, out: &mut [i16]) {
        out.fill(0);
        self.mix_into(out);
    }

    /// Add the mix of all voices to `out` (interleaved stereo), saturating.
    pub fn mix_into(&mut self, out: &mut [i16]) {
        let master = self.master as i64;
        for slot in &mut self.voices {
            let Some(v) = slot else { continue };
            let pan = v.params.pan.clamp(-PAN_RANGE, PAN_RANGE) as i64;
            let range = PAN_RANGE as i64;
            // Linear balance: centre leaves both channels at unity.
            let gl = v.params.volume as i64 * (range - pan.max(0)) / range;
            let gr = v.params.volume as i64 * (range + pan.min(0)) / range;
            let mut finished = false;
            for frame in out.chunks_exact_mut(CHANNELS) {
                let i = (v.pos >> 32) as usize;
                let Some(a) = v.frame(i) else {
                    finished = true;
                    break;
                };
                let b = v.frame(i + 1).unwrap_or(a);
                let t = ((v.pos & 0xffff_ffff) >> 16) as i64; // 16-bit fraction
                let l = a[0] as i64 + (((b[0] - a[0]) as i64 * t) >> 16);
                let r = a[1] as i64 + (((b[1] - a[1]) as i64 * t) >> 16);
                let l = (l * gl * master) >> 16;
                let r = (r * gr * master) >> 16;
                frame[0] = (frame[0] as i64 + l).clamp(i16::MIN as i64, i16::MAX as i64) as i16;
                frame[1] = (frame[1] as i64 + r).clamp(i16::MIN as i64, i16::MAX as i64) as i16;
                v.pos += v.step;
                if v.params.looping {
                    let len = (v.sample.frames() as u64) << 32;
                    if v.pos >= len {
                        v.pos -= len;
                    }
                }
            }
            if finished {
                *slot = None;
            }
        }
    }
}

/// Channels of the original mixer.
pub const SFX_CHANNELS: usize = 8;
/// Output rate of the original mixer; all `.PCM` samples are at this rate.
pub const SFX_RATE: u32 = 16_000;

#[derive(Clone)]
struct SfxChannel {
    data: Arc<[i16]>,
    /// Index of the next sample.
    cur: usize,
    volume: u8,
    /// Loops left; `u32::MAX` (the original's -1) is effectively endless.
    loop_count: u32,
}

/// The sound-effect mixer of `P.EXE` as documented in
/// `docs/re/engine/sound.md`: 8 channels of signed 16-bit mono samples at
/// 16000 Hz, no resampling or pitch, per-channel volume 0..=255, a master
/// volume, and the sum divided by `max(active channels, 4)`.
///
/// The mix itself (`mix_16m`, `0x2e6d0`) is reproduced sample for sample at
/// 16 kHz. Bringing that mono stream to the frontend's 44.1 kHz stereo is
/// this port's addition (linear interpolation); the original handed the
/// 16 kHz buffer to the sound card driver in 630-frame halves.
pub struct SfxMixer {
    channels: [Option<SfxChannel>; SFX_CHANNELS],
    master: i32,
    /// Two consecutive 16 kHz output samples and the position between them
    /// (0.32 fixed point) for the resampler.
    prev: i32,
    next: i32,
    frac: u64,
}

impl Default for SfxMixer {
    fn default() -> Self {
        Self::new()
    }
}

impl SfxMixer {
    pub fn new() -> Self {
        Self { channels: Default::default(), master: 256, prev: 0, next: 0, frac: 0 }
    }

    /// Master volume as the original stores it (`GAME.STP` volume x 16).
    pub fn set_master(&mut self, master: i32) {
        self.master = master;
    }

    /// `sfx_play` (`0x2ef10`): one-shot on the first idle channel of 1..=7,
    /// else on the one of 1..=7 with the fewest samples left. Returns the channel.
    pub fn play(&mut self, sample: &Arc<[i16]>, volume: u8) -> usize {
        let ch = (1..SFX_CHANNELS).find(|&c| self.channels[c].is_none()).unwrap_or_else(|| {
            (1..SFX_CHANNELS)
                .min_by_key(|&c| self.channels[c].as_ref().map_or(0, |v| v.data.len() - v.cur))
                .unwrap()
        });
        self.start(ch, sample, volume, 0);
        ch
    }

    /// `sfx_play_loop` (`0x2ef40`): endless loop on channel 0 (the sea ambience).
    pub fn play_loop(&mut self, sample: &Arc<[i16]>, volume: u8) {
        self.start(0, sample, volume, u32::MAX);
    }

    fn start(&mut self, ch: usize, sample: &Arc<[i16]>, volume: u8, loop_count: u32) {
        self.channels[ch] =
            (!sample.is_empty()).then(|| SfxChannel { data: sample.clone(), cur: 0, volume, loop_count });
    }

    /// `channel0_set_volume` (`0x2ef70`).
    pub fn set_channel0_volume(&mut self, volume: u8) {
        if let Some(c) = self.channels[0].as_mut() {
            c.volume = volume;
        }
    }

    pub fn is_active(&self, ch: usize) -> bool {
        self.channels.get(ch).is_some_and(Option::is_some)
    }

    pub fn active_channels(&self) -> usize {
        self.channels.iter().filter(|c| c.is_some()).count()
    }

    pub fn stop_all(&mut self) {
        self.channels = Default::default();
    }

    /// One output sample at 16 kHz, exactly as the original computes it.
    pub fn next_sample_16k(&mut self) -> i16 {
        let mut acc: i32 = 0;
        for slot in &mut self.channels {
            let Some(c) = slot else { continue };
            acc += (c.data[c.cur] as i32 * c.volume as i32) >> 8;
            c.cur += 1;
            // The descriptor's loop spans the whole sample (loop start 0,
            // loop length = length), so "end" and "loop end" coincide.
            if c.cur >= c.data.len() {
                if c.loop_count == 0 {
                    *slot = None;
                } else {
                    c.loop_count = c.loop_count.wrapping_sub(1);
                    c.cur = 0;
                }
            }
        }
        acc = acc.wrapping_mul(self.master) >> 8;
        // Active count is read after the channel loop, as in the original.
        let active = self.active_channels() as i32;
        acc /= active.max(4);
        acc as i16
    }

    /// Overwrite `out` (interleaved stereo at [`SAMPLE_RATE`]); what an
    /// `App::mix_audio` implementation calls.
    pub fn fill(&mut self, out: &mut [i16]) {
        const STEP: u64 = ((SFX_RATE as u64) << 32) / SAMPLE_RATE as u64;
        for frame in out.chunks_exact_mut(CHANNELS) {
            let t = (self.frac >> 16) as i32; // 16-bit fraction
            let v = self.prev + (((self.next - self.prev) * t) >> 16);
            frame[0] = v as i16;
            frame[1] = v as i16;
            self.frac += STEP;
            while self.frac >= 1 << 32 {
                self.frac -= 1 << 32;
                self.prev = self.next;
                self.next = self.next_sample_16k() as i32;
            }
        }
    }
}

/// Plays a Red Book audio track from the disc image, standing in for the CD
/// drive's analogue output. Driven by `Command::PlayCdTrack` / `Command::StopCd`.
pub struct CdPlayer {
    disc: Option<Rc<Disc>>,
    /// Current track number, if one is selected and exists.
    track: Option<u8>,
    /// Position in sample frames from the start of the track.
    pos: usize,
    looping: bool,
    playing: bool,
    volume: u16,
}

impl CdPlayer {
    /// `disc = None` (assets loaded from a directory) gives a player that stays silent.
    pub fn new(disc: Option<Rc<Disc>>) -> Self {
        // The mixer writes CD samples through unchanged; this only holds while
        // the CD and output rates match.
        const _: () = assert!(AUDIO_RATE == SAMPLE_RATE);
        Self { disc, track: None, pos: 0, looping: false, playing: false, volume: VOLUME_UNITY }
    }

    pub fn has_disc(&self) -> bool {
        self.disc.is_some()
    }

    /// Start `track` from its beginning. Returns `false` (and stops) if there
    /// is no disc or it has no such audio track.
    pub fn play(&mut self, track: u8, looping: bool) -> bool {
        let exists = self.disc.as_ref().is_some_and(|d| d.audio_track(track).is_some());
        self.track = exists.then_some(track);
        self.pos = 0;
        self.looping = looping;
        self.playing = exists;
        exists
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Track currently selected (still set after a non-looping track ended).
    pub fn track(&self) -> Option<u8> {
        self.track
    }

    /// Playback position in sample frames.
    pub fn position_frames(&self) -> usize {
        self.pos
    }

    /// `0..=VOLUME_UNITY`.
    pub fn set_volume(&mut self, volume: u16) {
        self.volume = volume;
    }

    /// Add the music to `out` (interleaved stereo at [`SAMPLE_RATE`]), saturating.
    pub fn mix_into(&mut self, out: &mut [i16]) {
        if !self.playing {
            return;
        }
        let (Some(disc), Some(number)) = (self.disc.as_ref(), self.track) else { return };
        let Some(track) = disc.audio_track(number) else { return };
        let len = track.len_frames();
        let vol = self.volume as i32;
        for frame in out.chunks_exact_mut(CHANNELS) {
            if self.pos >= len {
                if self.looping && len > 0 {
                    self.pos = 0;
                } else {
                    self.playing = false;
                    break;
                }
            }
            let [l, r] = track.frame(self.pos);
            self.pos += 1;
            let l = (l as i32 * vol) >> 8;
            let r = (r as i32 * vol) >> 8;
            frame[0] = (frame[0] as i32 + l).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            frame[1] = (frame[1] as i32 + r).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_voice_passes_through() {
        let mut m = Mixer::new(2);
        let s = Sample::mono(vec![100i16, 200, 300], SAMPLE_RATE);
        assert_eq!(m.play(&s, VoiceParams::default()), Some(0));
        let mut out = [0i16; 8];
        m.fill(&mut out);
        assert_eq!(out, [100, 100, 200, 200, 300, 300, 0, 0]);
        assert!(!m.is_playing(0));
    }

    #[test]
    fn half_rate_voice_is_interpolated() {
        let mut m = Mixer::new(1);
        let s = Sample::mono(vec![0i16, 1000], SAMPLE_RATE / 2);
        m.play(&s, VoiceParams::default());
        let mut out = [0i16; 8];
        m.fill(&mut out);
        assert_eq!(out, [0, 0, 500, 500, 1000, 1000, 1000, 1000]);
    }

    #[test]
    fn pan_and_volume() {
        let mut m = Mixer::new(1);
        let s = Sample::stereo(vec![1000i16, 1000], SAMPLE_RATE);
        m.play(&s, VoiceParams { volume: VOLUME_UNITY / 2, pan: -PAN_RANGE, looping: false });
        let mut out = [0i16; 2];
        m.fill(&mut out);
        assert_eq!(out, [500, 0]);
    }

    #[test]
    fn looping_and_voice_allocation() {
        let mut m = Mixer::new(1);
        let s = Sample::mono(vec![1i16, 2], SAMPLE_RATE);
        let p = VoiceParams { looping: true, ..VoiceParams::default() };
        assert_eq!(m.play(&s, p), Some(0));
        assert_eq!(m.play(&s, p), None);
        let mut out = [0i16; 10];
        m.fill(&mut out);
        assert_eq!(out, [1, 1, 2, 2, 1, 1, 2, 2, 1, 1]);
        assert!(m.is_playing(0));
        m.stop(0);
        m.fill(&mut out);
        assert_eq!(out, [0; 10]);
    }

    #[test]
    fn sfx_mixer_matches_the_original_formula() {
        let mut m = SfxMixer::new();
        let s: Arc<[i16]> = vec![4000i16, -4000, 8000].into();
        // One channel at full volume: (4000 * 255 >> 8) * 256 >> 8, then / 4.
        assert_eq!(m.play(&s, 255), 1);
        assert_eq!(m.next_sample_16k(), (((4000i32 * 255) >> 8) / 4) as i16);
        // Negative values: arithmetic shift, then division rounding toward zero.
        assert_eq!(m.next_sample_16k(), (((-4000i32 * 255) >> 8) / 4) as i16);
        // The last sample ends the channel before the divisor is read (still 4).
        assert_eq!(m.next_sample_16k(), (((8000i32 * 255) >> 8) / 4) as i16);
        assert_eq!(m.active_channels(), 0);
        assert_eq!(m.next_sample_16k(), 0);

        // More than four active channels divide by the active count.
        let long: Arc<[i16]> = vec![1000i16; 10].into();
        for _ in 0..6 {
            m.play(&long, 128);
        }
        assert_eq!(m.active_channels(), 6);
        assert_eq!(m.next_sample_16k(), ((6 * ((1000i32 * 128) >> 8)) / 6) as i16);

        // All of 1..=7 busy: the channel closest to its end is replaced.
        m.stop_all();
        for _ in 0..7 {
            m.play(&long, 255);
        }
        let short: Arc<[i16]> = vec![1i16; 2].into();
        m.channels[3].as_mut().unwrap().cur = 8;
        assert_eq!(m.play(&short, 255), 3);

        // The loop channel never ends and has its own volume control.
        m.stop_all();
        m.play_loop(&short, 0);
        m.set_channel0_volume(255);
        for _ in 0..7 {
            m.next_sample_16k();
        }
        assert!(m.is_active(0));
    }

    #[test]
    fn sfx_mixer_resamples_to_output_rate() {
        let mut m = SfxMixer::new();
        let s: Arc<[i16]> = vec![16000i16; SFX_RATE as usize / 10].into(); // 0.1 s
        m.play(&s, 255);
        let mut out = vec![0i16; SAMPLE_RATE as usize / 5 * 2]; // 0.2 s
        m.fill(&mut out);
        let level = ((16000i32 * 255) >> 8) / 4;
        let loud = out.chunks_exact(2).filter(|f| f[0] == level as i16 && f[1] == f[0]).count();
        // 0.1 s of signal at the output rate, give or take the interpolation edges.
        assert!((loud as i32 - SAMPLE_RATE as i32 / 10).abs() < 8, "{loud}");
        assert!(out[out.len() - 2..] == [0, 0]);
    }

    #[test]
    fn cd_player_without_disc_is_silent() {
        let mut cd = CdPlayer::new(None);
        assert!(!cd.play(2, true));
        let mut out = [5i16; 4];
        cd.mix_into(&mut out);
        assert_eq!(out, [5; 4]);
    }
}
