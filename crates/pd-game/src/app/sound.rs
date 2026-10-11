//! Sample bank and effect playback (`docs/re/engine/sound.md`).

use std::sync::Arc;

use pd_engine::AssetSource;
use pd_engine::audio::SfxMixer;
use pd_formats::bnk::Bnk;
use pd_formats::pcm::Pcm;

/// Sample names in id order (pointer table `0x41a84`).
pub const SAMPLE_NAMES: [&str; 29] = [
    "bonus11.pcm",
    "boom11.pcm",
    "duck11.pcm",
    "mer11.pcm",
    "monste11.pcm",
    "pack11.pcm",
    "zap11.pcm",
    "flopg-11.pcm",
    "flopd-11.pcm",
    "pas-g-11.pcm",
    "pas-d-11.pcm",
    "move11.pcm",
    "yes11.pcm",
    "lance11.pcm",
    "dollar.pcm",
    "gat11.pcm",
    "impact11.pcm",
    "mdead1.pcm",
    "mdead2.pcm",
    "mtouch1.pcm",
    "mtouch2.pcm",
    "punch11.pcm",
    "teleport.pcm",
    "weapon11.pcm",
    "zop11.pcm",
    "pose.pcm",
    "take.pcm",
    "good.pcm",
    "rire.pcm",
];

/// Menu cursor moved (`menu_input`, `0x25d6a`).
pub const SFX_MOVE: usize = 11;
/// Menu entry chosen (`0x25d81`).
pub const SFX_SELECT: usize = 12;
/// Sea ambience, looped on channel 0 (`0x1241d`).
pub const SFX_SEA: usize = 3;

pub struct Sound {
    samples: Vec<Arc<[i16]>>,
    mixer: SfxMixer,
    /// `mixer_on` (`0x51c2d8`, = `GAME.STP` sound on): the 70 Hz tick only
    /// mixes while this is set.
    on: bool,
    /// Ids of every `sfx_play` call, for tests and traces.
    pub log: Vec<(usize, u8)>,
}

impl Sound {
    /// `samples_load_all` (`0x2f020`). Missing samples stay empty (silent).
    pub fn load(assets: &dyn AssetSource) -> Self {
        let bank = assets.load("SAMPLE.BNK");
        let bnk = bank.as_deref().and_then(|d| Bnk::parse(d).ok());
        let samples = SAMPLE_NAMES
            .iter()
            .map(|name| {
                let pcm = bnk.as_ref().and_then(|b| b.get(name)).and_then(|d| Pcm::parse(d).ok());
                pcm.map_or_else(|| Arc::from(Vec::new()), |p| Arc::from(p.samples))
            })
            .collect();
        Self { samples, mixer: SfxMixer::new(), on: true, log: Vec::new() }
    }

    /// `sfx_play(id, volume)` (`0x2ef10`).
    pub fn play(&mut self, id: usize, volume: u8) {
        if self.log.len() < 4096 {
            self.log.push((id, volume));
        }
        if let Some(s) = self.samples.get(id) {
            self.mixer.play(s, volume);
        }
    }

    /// `sfx_play_loop(id, volume)` (`0x2ef40`): channel 0, endless.
    pub fn play_loop(&mut self, id: usize, volume: u8) {
        if let Some(s) = self.samples.get(id) {
            self.mixer.play_loop(s, volume);
        }
    }

    /// `channel0_set_volume` (`0x2ef70`).
    pub fn set_loop_volume(&mut self, volume: u8) {
        self.mixer.set_channel0_volume(volume);
    }

    /// `GAME.STP` sound switch and SFX volume (`0x51c2d8`, `0x51c2e8 = volume << 4`).
    pub fn configure(&mut self, on: bool, sfx_volume: u32) {
        self.on = on;
        self.mixer.set_master((sfx_volume << 4) as i32);
    }

    pub fn mix(&mut self, out: &mut [i16]) {
        if self.on {
            self.mixer.fill(out);
        } else {
            out.fill(0);
        }
    }
}
