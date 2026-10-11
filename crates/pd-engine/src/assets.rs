//! [`AssetSource`] implementations that need no filesystem.

use std::collections::HashMap;
use std::rc::Rc;

use pd_formats::disc::Disc;

use crate::AssetSource;

/// Normalise a name the way the lookups here compare them: upper case,
/// `/` separators, no leading `./` or `/`.
pub fn normalize_name(name: &str) -> String {
    let s = name.replace('\\', "/").to_ascii_uppercase();
    let mut s = s.as_str();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    s.trim_start_matches('/').to_string()
}

/// Files of the data track of an in-memory disc image.
///
/// On the original disc all game files live in the directory `PUNKY/` (the
/// only entry of the root directory), while the game asks for bare names such
/// as `BANQUE.BNK`. Names are therefore resolved relative to a base directory.
///
/// The image is shared (`Rc`) with the CD-music player, so the ~127 MB of
/// bytes exist once.
#[derive(Clone)]
pub struct DiscAssets {
    disc: Rc<Disc>,
    /// Empty, or a normalised directory path ending in `/`.
    base: String,
}

impl DiscAssets {
    /// Use the directory that holds `P.EXE` as base (the disc root if there
    /// is no such file).
    pub fn new(disc: Rc<Disc>) -> Self {
        let base = disc
            .files()
            .iter()
            .find_map(|f| f.path.strip_suffix("P.EXE").filter(|dir| dir.is_empty() || dir.ends_with('/')))
            .unwrap_or("")
            .to_string();
        Self { disc, base }
    }

    /// Resolve names relative to `base` (e.g. `"PUNKY"`; `""` = disc root).
    pub fn with_base(disc: Rc<Disc>, base: &str) -> Self {
        let mut base = normalize_name(base);
        if !base.is_empty() && !base.ends_with('/') {
            base.push('/');
        }
        Self { disc, base }
    }

    pub fn disc(&self) -> &Rc<Disc> {
        &self.disc
    }

    /// Base directory, empty or ending in `/`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Names (relative to the base directory) of all files below it.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.disc.files().iter().filter_map(|f| f.path.strip_prefix(self.base.as_str()))
    }
}

impl AssetSource for DiscAssets {
    fn load(&self, name: &str) -> Option<Vec<u8>> {
        self.disc.read_file(&format!("{}{}", self.base, normalize_name(name)))
    }
}

/// Files preloaded into memory by the frontend (e.g. an extracted directory).
#[derive(Default, Clone)]
pub struct MemAssets {
    files: HashMap<String, Vec<u8>>,
}

impl MemAssets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a file. `name` may contain a sub-directory (`SETUP/SETD.EXE`).
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.files.insert(normalize_name(name), data);
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Normalised names of all files, unordered.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl<N: AsRef<str>> FromIterator<(N, Vec<u8>)> for MemAssets {
    fn from_iter<T: IntoIterator<Item = (N, Vec<u8>)>>(iter: T) -> Self {
        let mut m = MemAssets::new();
        for (name, data) in iter {
            m.insert(name.as_ref(), data);
        }
        m
    }
}

impl AssetSource for MemAssets {
    fn load(&self, name: &str) -> Option<Vec<u8>> {
        self.files.get(&normalize_name(name)).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_assets_are_case_insensitive() {
        let m: MemAssets = [("Setup\\setd.exe", vec![1u8, 2]), ("P.EXE", vec![3])].into_iter().collect();
        assert_eq!(m.load("SETUP/SETD.EXE"), Some(vec![1, 2]));
        assert_eq!(m.load("p.exe"), Some(vec![3]));
        assert_eq!(m.load("missing"), None);
        assert_eq!(m.len(), 2);
    }
}
