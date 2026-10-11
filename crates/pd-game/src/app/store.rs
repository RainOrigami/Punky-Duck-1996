//! Settings and save slots on [`Storage`], in the original's file formats.
//!
//! - `GAME.STP`: 8 x u32 (`settings_load` `0x258c0`, `settings_save` `0x25a40`).
//! - `SAVEGAME.REC`: 8 slot names of 10 bytes (`0x258c0`, written by `0x26100`).
//! - `SAVEGAME.__n`: one save slot (`0x26050` / `0x26100`), content owned by the simulation.
//! - `PORT.CFG`: this port's own switch (original / enhanced); kept out of
//!   `GAME.STP` so that file stays byte-compatible with the original.

use pd_engine::{AssetSource, Storage};
use pd_formats::stp::Stp;

pub const SLOTS: usize = 8;
pub const SLOT_NAME_LEN: usize = 10;

pub const KEY_STP: &str = "GAME.STP";
pub const KEY_REC: &str = "SAVEGAME.REC";
pub const KEY_PORT: &str = "PORT.CFG";

/// Storage key of save slot `n` (`SAVEGAME.__0` with the digit patched, `0x26050`).
pub fn slot_key(slot: usize) -> String {
    format!("SAVEGAME.__{slot}")
}

/// The variables behind the options menus.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `GAME.STP` contents (`0x41610`..`0x41624`, `0x416e4`, `0x4000c`).
    pub stp: Stp,
    /// Save slot names (`0x41630`, 8 x 10 bytes, NUL-terminated).
    pub slots: [[u8; SLOT_NAME_LEN]; SLOTS],
    /// View mode `0x4002c`: 0 VGA 320x200, 1 SVGA 320x200, 2 SVGA 640x400. Not saved.
    pub view_mode: u32,
    /// "FAST MODE" (interlace) `0x40044`. Not saved.
    pub fast_mode: u32,
    /// Mouse turning speed 0..=15 of the modern controls (`PORT.CFG`, `mouse=`).
    pub mouse_speed: u32,
}

pub const MOUSE_SPEED_DEFAULT: u32 = 7;

/// Value of `key=` in `PORT.CFG`.
fn port_get(storage: &dyn Storage, key: &str) -> Option<String> {
    let text = String::from_utf8(storage.read(KEY_PORT)?).ok()?;
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=').map(|v| v.trim().to_string()))
}

/// Set `key=value` in `PORT.CFG`, keeping its other lines.
fn port_set(storage: &mut dyn Storage, key: &str, value: &str) {
    let old = storage.read(KEY_PORT).and_then(|d| String::from_utf8(d).ok()).unwrap_or_default();
    let mut out = String::new();
    let mut done = false;
    for l in old.lines() {
        if l.strip_prefix(key).is_some_and(|r| r.starts_with('=')) {
            if !done {
                out.push_str(&format!("{key}={value}\n"));
                done = true;
            }
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !done {
        out.push_str(&format!("{key}={value}\n"));
    }
    storage.write(KEY_PORT, out.as_bytes());
}

impl Settings {
    /// The values compiled into `P.EXE` (used when there is no `GAME.STP`).
    pub fn builtin() -> Self {
        let mut slots = [[0u8; SLOT_NAME_LEN]; SLOTS];
        for (i, s) in slots.iter_mut().enumerate() {
            let name = format!("FREE {}", i + 1);
            s[..name.len()].copy_from_slice(name.as_bytes());
        }
        Self {
            stp: Stp {
                sound_on: 1,
                music_on: 1,
                controller: 0,
                sfx_volume: 15,
                cd_volume: 5,
                difficulty: 0,
                language: 0,
                game_speed: 0,
            },
            slots,
            view_mode: 2,
            fast_mode: 0,
            mouse_speed: MOUSE_SPEED_DEFAULT,
        }
    }

    /// `settings_load` (`0x258c0`): read `GAME.STP` and `SAVEGAME.REC` if
    /// present. The original reads them from its installation directory,
    /// which starts as a copy of the CD; here storage comes first and the
    /// disc's `GAME.STP` is the fallback.
    pub fn load(&mut self, storage: &dyn Storage, assets: &dyn AssetSource) {
        if let Some(stp) = storage.read(KEY_STP).or_else(|| assets.load(KEY_STP)).and_then(|d| Stp::parse(&d).ok()) {
            self.stp = stp;
        }
        if let Some(rec) = storage.read(KEY_REC) {
            // fread of 10 bytes per name; a short file leaves the rest as it was.
            for (slot, chunk) in self.slots.iter_mut().zip(rec.chunks_exact(SLOT_NAME_LEN)) {
                slot.copy_from_slice(chunk);
            }
        }
        if let Some(v) = port_get(storage, "mouse").and_then(|v| v.parse::<u32>().ok()) {
            self.mouse_speed = v.min(15);
        }
    }

    /// Write the port's own options to `PORT.CFG` if they differ from what is stored.
    pub fn save_port(&self, storage: &mut dyn Storage) {
        let stored = port_get(storage, "mouse").and_then(|v| v.parse::<u32>().ok()).unwrap_or(MOUSE_SPEED_DEFAULT);
        if stored != self.mouse_speed {
            port_set(storage, "mouse", &self.mouse_speed.to_string());
        }
    }

    /// `settings_save` (`0x25a40`).
    pub fn save_stp(&self, storage: &mut dyn Storage) {
        storage.write(KEY_STP, &self.stp.to_bytes());
    }

    /// The `SAVEGAME.REC` write at the end of `0x26100`.
    pub fn save_rec(&self, storage: &mut dyn Storage) {
        let bytes: Vec<u8> = self.slots.iter().flatten().copied().collect();
        storage.write(KEY_REC, &bytes);
    }

    /// Slot name as a C string.
    pub fn slot_name(&self, slot: usize) -> &[u8] {
        let s = &self.slots[slot];
        &s[..s.iter().position(|&b| b == 0).unwrap_or(SLOT_NAME_LEN)]
    }

    /// `strcpy` into the 10-byte slot name.
    pub fn set_slot_name(&mut self, slot: usize, name: &[u8]) {
        let s = &mut self.slots[slot];
        let n = name.len().min(SLOT_NAME_LEN - 1);
        s[..n].copy_from_slice(&name[..n]);
        s[n] = 0;
    }
}

/// Which behaviour set runs (`pd_game::settings::Enhancements`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Original,
    Enhanced,
}

impl Mode {
    pub fn enhancements(self) -> crate::settings::Enhancements {
        match self {
            Mode::Original => crate::settings::Enhancements::original(),
            Mode::Enhanced => crate::settings::Enhancements::enhanced(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Original => "original",
            Mode::Enhanced => "enhanced",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim() {
            "original" => Some(Mode::Original),
            "enhanced" => Some(Mode::Enhanced),
            _ => None,
        }
    }

    /// The stored choice (`PORT.CFG`, a text line `mode=original|enhanced`);
    /// original when nothing valid is stored.
    pub fn load(storage: &dyn Storage) -> Mode {
        port_get(storage, "mode").and_then(|v| Mode::parse(&v)).unwrap_or_default()
    }

    pub fn store(self, storage: &mut dyn Storage) {
        port_set(storage, "mode", self.name());
    }

    /// The flags for this mode: all off, or all on except the ones the user
    /// switched off (`PORT.CFG`, `off=` with field names separated by commas).
    pub fn stored_enhancements(self, storage: &dyn Storage) -> crate::settings::Enhancements {
        match self {
            Mode::Original => crate::settings::Enhancements::original(),
            Mode::Enhanced => crate::settings::Enhancements::enhanced_without(disabled_flags(storage).iter().map(String::as_str)),
        }
    }

    /// Frontend start-up rule: an explicit choice (CLI flag, page control)
    /// wins and is remembered; otherwise the remembered one applies.
    pub fn select(explicit: Option<Mode>, storage: &mut dyn Storage) -> Mode {
        match explicit {
            Some(m) => {
                if Mode::load(storage) != m || storage.read(KEY_PORT).is_none() {
                    m.store(storage);
                }
                m
            }
            None => Mode::load(storage),
        }
    }
}

/// Names of the enhancements switched off (`off=` in `PORT.CFG`).
pub fn disabled_flags(storage: &dyn Storage) -> Vec<String> {
    port_get(storage, "off")
        .map(|v| v.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect())
        .unwrap_or_default()
}

/// Remember the enhancements to leave off; unknown names are dropped.
pub fn store_disabled_flags(storage: &mut dyn Storage, names: &str) {
    let known: Vec<&str> = names
        .split(',')
        .map(str::trim)
        .filter(|n| crate::settings::FLAGS.iter().any(|(f, _, _)| f == n))
        .collect();
    let value = known.join(",");
    if port_get(storage, "off").unwrap_or_default() != value {
        port_set(storage, "off", &value);
    }
}

/// In-memory [`Storage`] (tests, headless runs).
#[derive(Default, Clone)]
pub struct MemStorage {
    pub files: std::collections::BTreeMap<String, Vec<u8>>,
}

impl Storage for MemStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        self.files.get(key).cloned()
    }

    fn write(&mut self, key: &str, data: &[u8]) {
        self.files.insert(key.to_string(), data.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pd_engine::assets::MemAssets;

    #[test]
    fn stp_and_rec_round_trip() {
        let mut st = MemStorage::default();
        let mut s = Settings::builtin();
        s.stp.difficulty = 2;
        s.set_slot_name(3, b"ISLAND01");
        s.save_stp(&mut st);
        s.save_rec(&mut st);
        assert_eq!(st.files[KEY_STP].len(), 32);
        assert_eq!(st.files[KEY_REC].len(), 80);
        assert_eq!(&st.files[KEY_REC][..7], b"FREE 1\0");
        let mut t = Settings::builtin();
        t.load(&st, &MemAssets::new());
        assert_eq!(t, s);
        assert_eq!(t.slot_name(3), b"ISLAND01");
    }

    #[test]
    fn mode_is_stored_outside_game_stp() {
        let mut st = MemStorage::default();
        assert_eq!(Mode::select(None, &mut st), Mode::Original);
        assert!(st.files.is_empty());
        assert_eq!(Mode::select(Some(Mode::Enhanced), &mut st), Mode::Enhanced);
        assert_eq!(Mode::select(None, &mut st), Mode::Enhanced);
        assert_eq!(st.files.keys().collect::<Vec<_>>(), [KEY_PORT]);
        assert_eq!(Mode::select(Some(Mode::Original), &mut st), Mode::Original);
        assert_eq!(Mode::load(&st), Mode::Original);
    }

    #[test]
    fn single_enhancements_can_be_switched_off() {
        let mut st = MemStorage::default();
        Mode::Enhanced.store(&mut st);
        assert_eq!(Mode::Enhanced.stored_enhancements(&st), crate::settings::Enhancements::enhanced());
        store_disabled_flags(&mut st, "minimap, far_view,bogus");
        assert_eq!(disabled_flags(&st), ["minimap", "far_view"]);
        let e = Mode::Enhanced.stored_enhancements(&st);
        assert!(!e.minimap && !e.far_view && e.smooth_view);
        assert_eq!(Mode::Original.stored_enhancements(&st), crate::settings::Enhancements::original());
    }

    #[test]
    fn mouse_speed_shares_port_cfg_with_the_mode() {
        let mut st = MemStorage::default();
        Mode::Enhanced.store(&mut st);
        let mut s = Settings::builtin();
        s.save_port(&mut st);
        assert_eq!(st.files[KEY_PORT], b"mode=enhanced\n", "default is not written");
        s.mouse_speed = 12;
        s.save_port(&mut st);
        Mode::Original.store(&mut st);
        assert_eq!(st.files[KEY_PORT], b"mode=original\nmouse=12\n");
        let mut t = Settings::builtin();
        t.load(&st, &MemAssets::new());
        assert_eq!(t.mouse_speed, 12);
    }
}
