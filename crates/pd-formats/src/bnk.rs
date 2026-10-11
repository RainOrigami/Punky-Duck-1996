//! `.BNK` archives (`BANQUE.BNK`, `PICTURE.BNK`, `SAMPLE.BNK`).
//!
//! Spec: `docs/re/formats/bnk.md`. Follows the game's lookup routine at
//! `P.EXE` `0x2b4f0`: `u32 count`, then `count` 20-byte records
//! { `char name[12]`, `u32 offset`, `u32 size` }, offsets absolute.

use std::fmt;

/// Size of one directory record.
pub const ENTRY_SIZE: usize = 20;
/// Size of the name field. Names of exactly 12 characters have no NUL.
pub const NAME_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BnkError(pub String);

impl fmt::Display for BnkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BNK: {}", self.0)
    }
}

impl std::error::Error for BnkError {}

/// One directory record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BnkEntry {
    /// Name as stored (upper case 8.3 on the original disc).
    pub name: String,
    /// Absolute offset of the member inside the archive.
    pub offset: u32,
    pub size: u32,
}

/// A parsed archive borrowing the archive bytes.
#[derive(Debug, Clone)]
pub struct Bnk<'a> {
    data: &'a [u8],
    entries: Vec<BnkEntry>,
}

/// ASCII-only case folding, as done by the game's compare routine
/// (`0x35b18` lowers `A`..`Z` only).
fn fold(b: u8) -> u8 {
    if b.is_ascii_uppercase() { b + 0x20 } else { b }
}

/// The game's name comparison: case-insensitive compare of at most
/// [`NAME_LEN`] bytes, stopping at a NUL in both strings.
fn name_matches(stored: &[u8], wanted: &[u8]) -> bool {
    for i in 0..NAME_LEN {
        let a = stored.get(i).copied().unwrap_or(0);
        let b = wanted.get(i).copied().unwrap_or(0);
        if fold(a) != fold(b) {
            return false;
        }
        if a == 0 {
            return true;
        }
    }
    true
}

impl<'a> Bnk<'a> {
    /// Parses the directory and checks that every member lies inside `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, BnkError> {
        if data.len() < 4 {
            return Err(BnkError("file shorter than the count field".into()));
        }
        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let dir_end = count
            .checked_mul(ENTRY_SIZE)
            .and_then(|n| n.checked_add(4))
            .filter(|&n| n <= data.len())
            .ok_or_else(|| BnkError(format!("directory of {count} entries exceeds file")))?;
        let mut entries = Vec::with_capacity(count);
        for rec in data[4..dir_end].chunks_exact(ENTRY_SIZE) {
            let raw = &rec[..NAME_LEN];
            let len = raw.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
            let name: String = raw[..len].iter().map(|&b| b as char).collect();
            let offset = u32::from_le_bytes([rec[12], rec[13], rec[14], rec[15]]);
            let size = u32::from_le_bytes([rec[16], rec[17], rec[18], rec[19]]);
            let end = (offset as usize).checked_add(size as usize);
            if end.is_none_or(|e| e > data.len()) {
                return Err(BnkError(format!("member {name} lies outside the archive")));
            }
            entries.push(BnkEntry { name, offset, size });
        }
        Ok(Self { data, entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Directory records in file order.
    pub fn entries(&self) -> &[BnkEntry] {
        &self.entries
    }

    /// Bytes of a directory record's member.
    pub fn bytes(&self, entry: &BnkEntry) -> &'a [u8] {
        &self.data[entry.offset as usize..entry.offset as usize + entry.size as usize]
    }

    /// Finds the first member whose name matches, the way the game does
    /// (linear scan, ASCII case-insensitive, first 12 characters).
    pub fn find(&self, name: &str) -> Option<&BnkEntry> {
        let wanted = name.as_bytes();
        self.entries.iter().find(|e| name_matches(e.name.as_bytes(), wanted))
    }

    /// Member bytes by name; see [`Bnk::find`].
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        self.find(name).map(|e| self.bytes(e))
    }

    /// Iterates `(name, bytes)` in directory order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &'a [u8])> + '_ {
        self.entries.iter().map(|e| (e.name.as_str(), self.bytes(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&2u32.to_le_bytes());
        let mut rec = |name: &[u8], off: u32, size: u32| {
            let mut n = [0u8; NAME_LEN];
            n[..name.len()].copy_from_slice(name);
            v.extend_from_slice(&n);
            v.extend_from_slice(&off.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
        };
        rec(b"A.SPT", 44, 3);
        rec(b"TRANS1-2.RON", 47, 2);
        v.extend_from_slice(b"abcde");
        v
    }

    #[test]
    fn parses_and_looks_up() {
        let data = sample();
        let bnk = Bnk::parse(&data).unwrap();
        assert_eq!(bnk.len(), 2);
        assert_eq!(bnk.get("a.spt"), Some(&b"abc"[..]));
        assert_eq!(bnk.get("Trans1-2.ron"), Some(&b"de"[..]));
        // Only the first 12 characters take part in the comparison.
        assert_eq!(bnk.get("TRANS1-2.RONX"), Some(&b"de"[..]));
        assert_eq!(bnk.get("A.SP"), None);
        assert_eq!(bnk.get("A.SPTX"), None);
        let names: Vec<_> = bnk.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(names, ["A.SPT", "TRANS1-2.RON"]);
    }

    #[test]
    fn rejects_out_of_range() {
        let mut data = sample();
        data.truncate(48);
        assert!(Bnk::parse(&data).is_err());
        assert!(Bnk::parse(&[1, 0]).is_err());
        assert!(Bnk::parse(&[9, 0, 0, 0]).is_err());
    }

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    #[test]
    fn disc_archives() {
        for (file, count, ext) in
            [("BANQUE.BNK", 30, ".SPT"), ("PICTURE.BNK", 54, ".RON"), ("SAMPLE.BNK", 29, ".PCM")]
        {
            let Some(data) = disc(file) else {
                eprintln!("skipping: {file} not present");
                return;
            };
            let bnk = Bnk::parse(&data).unwrap();
            assert_eq!(bnk.len(), count, "{file}");
            assert!(bnk.entries().iter().all(|e| e.name.ends_with(ext)), "{file}");
            // Members are stored back to back, in directory order, to the end of the file.
            let mut pos = 4 + count * ENTRY_SIZE;
            for e in bnk.entries() {
                assert_eq!(e.offset as usize, pos, "{file} {}", e.name);
                pos += e.size as usize;
            }
            assert_eq!(pos, data.len(), "{file}");
        }
        let data = disc("PICTURE.BNK").unwrap();
        let bnk = Bnk::parse(&data).unwrap();
        assert!(bnk.get("Ciel_1.RON").is_some());
        assert!(bnk.get("trans1-2.ron").is_some());
    }
}
