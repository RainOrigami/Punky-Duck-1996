//! Filesystem side of asset loading and persistent storage (native only; the
//! core crates never touch `std::fs`).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use pd_engine::Storage;
use pd_engine::assets::MemAssets;
use pd_formats::disc::Disc;

/// Load a disc image given the path of either its `.cue` or its `.bin`.
pub fn load_disc(path: &Path) -> Result<Rc<Disc>, String> {
    let is_cue = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"));
    let (cue_path, bin_hint) = if is_cue {
        (path.to_path_buf(), None)
    } else {
        // Given the .bin: the cue sits next to it with the same stem.
        let cue = ["cue", "CUE"]
            .iter()
            .map(|ext| path.with_extension(ext))
            .find(|p| p.is_file())
            .ok_or_else(|| format!("no .cue file next to {}", path.display()))?;
        (cue, Some(path.to_path_buf()))
    };
    let cue_text = std::fs::read_to_string(&cue_path).map_err(|e| format!("{}: {e}", cue_path.display()))?;
    let bin_path = match bin_hint {
        Some(p) => p,
        None => {
            // The cue names its bin file relative to its own directory.
            let cue = pd_formats::disc::parse_cue(&cue_text).map_err(|e| e.to_string())?;
            let dir = cue_path.parent().unwrap_or(Path::new("."));
            let named = dir.join(&cue.file);
            if named.is_file() {
                named
            } else {
                ["bin", "BIN"]
                    .iter()
                    .map(|ext| cue_path.with_extension(ext))
                    .find(|p| p.is_file())
                    .ok_or_else(|| format!("bin file {} not found", named.display()))?
            }
        }
    };
    let bin = std::fs::read(&bin_path).map_err(|e| format!("{}: {e}", bin_path.display()))?;
    Disc::new(bin, &cue_text).map(Rc::new).map_err(|e| e.to_string())
}

/// Preload every file below `dir` (an extracted `PUNKY` directory).
pub fn load_dir(dir: &Path) -> Result<MemAssets, String> {
    fn walk(dir: &Path, prefix: &str, out: &mut MemAssets) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, &format!("{prefix}{name}/"), out)?;
            } else {
                out.insert(&format!("{prefix}{name}"), std::fs::read(&path)?);
            }
        }
        Ok(())
    }
    let mut assets = MemAssets::new();
    walk(dir, "", &mut assets).map_err(|e| format!("{}: {e}", dir.display()))?;
    if assets.is_empty() {
        return Err(format!("{}: no files", dir.display()));
    }
    Ok(assets)
}

/// [`Storage`] as one file per key under `$XDG_DATA_HOME/punkyduck/`
/// (default `~/.local/share/punkyduck/`).
pub struct FileStorage {
    dir: PathBuf,
}

impl FileStorage {
    pub fn new() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        Self { dir: base.join("punkyduck") }
    }

    /// Storage in a given directory (`--storage`).
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Keys become file names; anything but `[A-Za-z0-9._-]` is replaced.
    fn path(&self, key: &str) -> PathBuf {
        let name: String =
            key.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
        self.dir.join(if name.trim_matches('.').is_empty() { "_".to_string() } else { name })
    }
}

impl Storage for FileStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(key)).ok()
    }

    fn write(&mut self, key: &str, data: &[u8]) {
        let result = std::fs::create_dir_all(&self.dir).and_then(|_| std::fs::write(self.path(key), data));
        if let Err(e) = result {
            eprintln!("pd-native: cannot save {key}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_storage_round_trip() {
        let dir = std::env::temp_dir().join(format!("pd-native-test-{}", std::process::id()));
        let mut s = FileStorage::at(dir.clone());
        assert_eq!(s.read("GAME.STP"), None);
        s.write("GAME.STP", &[1, 2, 3]);
        assert_eq!(s.read("GAME.STP"), Some(vec![1, 2, 3]));
        s.write("../evil", &[9]);
        assert!(dir.join(".._evil").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
