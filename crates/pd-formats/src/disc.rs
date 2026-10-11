//! In-memory reader for the original disc image (`.bin` + `.cue`).
//!
//! Spec: `docs/re/formats/disc.md`. The image is one raw 2352-byte-per-sector
//! file: track 1 is a MODE2/2352 ISO9660 data track, tracks 2..=9 are Red Book
//! audio (44.1 kHz, stereo, signed 16-bit little-endian).
//!
//! Pure: the caller supplies the bytes of both files.

use std::collections::HashMap;
use std::fmt;

/// Raw sector size of every track in a `*/2352` bin file.
pub const RAW_SECTOR: usize = 2352;
/// ISO9660 logical block size (user data bytes per data sector).
pub const USER_DATA: usize = 2048;
/// CD frames (sectors) per second.
pub const FRAMES_PER_SECOND: u32 = 75;
/// Sample rate of Red Book audio.
pub const AUDIO_RATE: u32 = 44_100;
/// Stereo sample frames per CD sector (2352 / 4).
pub const SAMPLE_FRAMES_PER_SECTOR: usize = RAW_SECTOR / 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscError(pub String);

impl fmt::Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "disc image: {}", self.0)
    }
}

impl std::error::Error for DiscError {}

fn err<T>(msg: impl Into<String>) -> Result<T, DiscError> {
    Err(DiscError(msg.into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// `MODE1/2352`: 16 bytes sync+header, 2048 bytes user data.
    Mode1Raw,
    /// `MODE2/2352`: 16 bytes sync+header, 8 bytes XA subheader, 2048 bytes user data (form 1).
    Mode2Raw,
    /// `AUDIO`: 588 stereo i16 LE sample frames per sector.
    Audio,
}

impl TrackKind {
    /// Offset of the 2048 user-data bytes inside a raw sector (data tracks only).
    pub fn user_data_offset(self) -> Option<usize> {
        match self {
            TrackKind::Mode1Raw => Some(16),
            TrackKind::Mode2Raw => Some(24),
            TrackKind::Audio => None,
        }
    }
}

/// One `TRACK` of the cue sheet. Positions are sectors from the start of the bin file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub number: u8,
    pub kind: TrackKind,
    /// `INDEX 00` (pregap stored in the file), if present.
    pub index0: Option<u32>,
    /// `INDEX 01`: where the track's content starts.
    pub start: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueSheet {
    /// Name given in the `FILE` line (informational; the caller supplies the bytes).
    pub file: String,
    pub tracks: Vec<Track>,
}

/// Parse `mm:ss:ff` into a sector count.
fn parse_msf(s: &str) -> Option<u32> {
    let mut it = s.split(':');
    let m: u32 = it.next()?.parse().ok()?;
    let sec: u32 = it.next()?.parse().ok()?;
    let f: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() || sec >= 60 || f >= FRAMES_PER_SECOND {
        return None;
    }
    Some((m * 60 + sec) * FRAMES_PER_SECOND + f)
}

/// Parse a single-file cue sheet. Unsupported constructs (several `FILE`s,
/// non-2352 track modes, non-binary files) are errors rather than guesses.
pub fn parse_cue(text: &str) -> Result<CueSheet, DiscError> {
    let mut file: Option<String> = None;
    let mut tracks: Vec<Track> = Vec::new();
    // Track being built: (number, kind, index0, index1)
    let mut cur: Option<(u8, TrackKind, Option<u32>, Option<u32>)> = None;

    fn finish(
        cur: &mut Option<(u8, TrackKind, Option<u32>, Option<u32>)>,
        tracks: &mut Vec<Track>,
    ) -> Result<(), DiscError> {
        if let Some((number, kind, index0, index1)) = cur.take() {
            let Some(start) = index1 else {
                return err(format!("track {number} has no INDEX 01"));
            };
            tracks.push(Track { number, kind, index0, start });
        }
        Ok(())
    }

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        let mut words = line.split_whitespace();
        let Some(keyword) = words.next() else { continue };
        match keyword.to_ascii_uppercase().as_str() {
            "FILE" => {
                if file.is_some() {
                    return err("cue sheets with more than one FILE are not supported");
                }
                let rest = line[keyword.len()..].trim();
                let (name, kind) = if let Some(stripped) = rest.strip_prefix('"') {
                    let Some(end) = stripped.find('"') else {
                        return err(format!("line {}: unterminated FILE name", lineno + 1));
                    };
                    (&stripped[..end], stripped[end + 1..].trim())
                } else {
                    rest.split_once(char::is_whitespace).unwrap_or((rest, ""))
                };
                if !kind.trim().eq_ignore_ascii_case("BINARY") {
                    return err(format!("FILE type {kind:?} is not supported (need BINARY)"));
                }
                file = Some(name.to_string());
            }
            "TRACK" => {
                finish(&mut cur, &mut tracks)?;
                let number = words.next().and_then(|w| w.parse::<u8>().ok());
                let mode = words.next().map(|w| w.to_ascii_uppercase());
                let (Some(number), Some(mode)) = (number, mode) else {
                    return err(format!("line {}: malformed TRACK", lineno + 1));
                };
                let kind = match mode.as_str() {
                    "AUDIO" => TrackKind::Audio,
                    "MODE1/2352" => TrackKind::Mode1Raw,
                    "MODE2/2352" => TrackKind::Mode2Raw,
                    other => return err(format!("track mode {other} is not supported")),
                };
                cur = Some((number, kind, None, None));
            }
            "INDEX" => {
                let idx = words.next().and_then(|w| w.parse::<u8>().ok());
                let pos = words.next().and_then(parse_msf);
                let (Some(idx), Some(pos)) = (idx, pos) else {
                    return err(format!("line {}: malformed INDEX", lineno + 1));
                };
                let Some(t) = cur.as_mut() else {
                    return err(format!("line {}: INDEX outside TRACK", lineno + 1));
                };
                match idx {
                    0 => t.2 = Some(pos),
                    1 => t.3 = Some(pos),
                    _ => {} // sub-indices carry no layout information
                }
            }
            // No effect on the file layout (PREGAP/POSTGAP are not stored in the file).
            "REM" | "PREGAP" | "POSTGAP" | "FLAGS" | "TITLE" | "PERFORMER" | "SONGWRITER"
            | "CATALOG" | "ISRC" | "CDTEXTFILE" => {}
            other => return err(format!("line {}: unknown cue keyword {other}", lineno + 1)),
        }
    }
    finish(&mut cur, &mut tracks)?;

    let Some(file) = file else { return err("cue sheet has no FILE") };
    if tracks.is_empty() {
        return err("cue sheet has no tracks");
    }
    for pair in tracks.windows(2) {
        if pair[1].start <= pair[0].start || pair[1].index0.is_some_and(|p| p < pair[0].start) {
            return err("cue tracks are not in ascending order");
        }
    }
    Ok(CueSheet { file, tracks })
}

/// A file of the ISO9660 filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Upper-case path with `/` separators and without the `;1` version suffix,
    /// e.g. `"P.EXE"` or `"SETUP/SETD.EXE"`.
    pub path: String,
    /// First logical block (sector relative to the start of the data track).
    pub lba: u32,
    /// Size in bytes.
    pub size: u32,
}

/// A Red Book audio track: 44.1 kHz stereo signed 16-bit little-endian PCM.
///
/// The image bytes are not guaranteed to be 2-byte aligned, so samples are
/// exposed through accessors over the borrowed byte slice instead of a `&[i16]`.
#[derive(Clone, Copy)]
pub struct AudioTrack<'a> {
    bytes: &'a [u8],
}

impl<'a> AudioTrack<'a> {
    /// Raw little-endian sample bytes (`L lo, L hi, R lo, R hi` per frame).
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Number of stereo sample frames.
    pub fn len_frames(&self) -> usize {
        self.bytes.len() / 4
    }

    /// Number of CD sectors.
    pub fn len_sectors(&self) -> usize {
        self.bytes.len() / RAW_SECTOR
    }

    /// One stereo frame `[left, right]`.
    pub fn frame(&self, i: usize) -> [i16; 2] {
        let b = &self.bytes[i * 4..i * 4 + 4];
        [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]])]
    }

    /// Copy interleaved stereo samples starting at sample frame `start` into
    /// `out`; returns the number of sample frames written (short at track end).
    pub fn read(&self, start: usize, out: &mut [i16]) -> usize {
        let avail = self.len_frames().saturating_sub(start);
        let n = avail.min(out.len() / 2);
        let src = &self.bytes[start.min(self.len_frames()) * 4..];
        for (dst, s) in out.chunks_exact_mut(2).zip(src.chunks_exact(4)).take(n) {
            dst[0] = i16::from_le_bytes([s[0], s[1]]);
            dst[1] = i16::from_le_bytes([s[2], s[3]]);
        }
        n
    }

    /// All samples, interleaved.
    pub fn to_vec(&self) -> Vec<i16> {
        self.bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }
}

/// The disc image held in memory.
pub struct Disc {
    bin: Vec<u8>,
    cue: CueSheet,
    /// Total sectors in the bin file.
    sectors: u32,
    files: Vec<FileEntry>,
    by_name: HashMap<String, usize>,
    volume_id: String,
}

fn normalize(name: &str) -> String {
    let mut s = name.replace('\\', "/").to_ascii_uppercase();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    s.trim_start_matches('/').to_string()
}

impl Disc {
    /// Take ownership of the `.bin` bytes and parse the `.cue` text.
    pub fn new(bin: Vec<u8>, cue_text: &str) -> Result<Self, DiscError> {
        let cue = parse_cue(cue_text)?;
        if bin.len() % RAW_SECTOR != 0 {
            return err(format!("bin size {} is not a multiple of {RAW_SECTOR}", bin.len()));
        }
        let sectors = (bin.len() / RAW_SECTOR) as u32;
        if cue.tracks.last().unwrap().start >= sectors {
            return err("cue sheet describes more data than the bin file holds");
        }
        let mut disc = Disc {
            bin,
            cue,
            sectors,
            files: Vec::new(),
            by_name: HashMap::new(),
            volume_id: String::new(),
        };
        if disc.cue.tracks[0].kind != TrackKind::Audio {
            disc.read_filesystem()?;
        }
        Ok(disc)
    }

    pub fn cue(&self) -> &CueSheet {
        &self.cue
    }

    /// ISO9660 volume identifier (`"PUNKY DUCK"` on the original disc).
    pub fn volume_id(&self) -> &str {
        &self.volume_id
    }

    /// Sector range `[start, end)` of track `number`'s content (from its INDEX 01
    /// up to the next track's INDEX 00/01, or the end of the file).
    pub fn track_sectors(&self, number: u8) -> Option<(u32, u32)> {
        let i = self.cue.tracks.iter().position(|t| t.number == number)?;
        let start = self.cue.tracks[i].start;
        let end = match self.cue.tracks.get(i + 1) {
            Some(next) => next.index0.unwrap_or(next.start),
            None => self.sectors,
        };
        Some((start, end))
    }

    /// Track numbers of all audio tracks, ascending.
    pub fn audio_track_numbers(&self) -> Vec<u8> {
        self.cue.tracks.iter().filter(|t| t.kind == TrackKind::Audio).map(|t| t.number).collect()
    }

    /// Audio track by cue track number (2..=9 on the original disc).
    pub fn audio_track(&self, number: u8) -> Option<AudioTrack<'_>> {
        let t = self.cue.tracks.iter().find(|t| t.number == number)?;
        if t.kind != TrackKind::Audio {
            return None;
        }
        let (start, end) = self.track_sectors(number)?;
        Some(AudioTrack { bytes: &self.bin[start as usize * RAW_SECTOR..end as usize * RAW_SECTOR] })
    }

    /// All files of the data track, in directory order (directories themselves are not listed).
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }

    /// Case-insensitive lookup; `/` or `\` separate directories.
    pub fn file(&self, name: &str) -> Option<&FileEntry> {
        self.by_name.get(&normalize(name)).map(|&i| &self.files[i])
    }

    /// Read a whole file by name.
    pub fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.file(name)?;
        self.read_extent(e.lba, e.size as usize).ok()
    }

    /// 2048 user-data bytes of logical block `lba` of the data track.
    fn block(&self, lba: u32) -> Result<&[u8], DiscError> {
        let t = &self.cue.tracks[0];
        let Some(off) = t.kind.user_data_offset() else {
            return err("track 1 is not a data track");
        };
        let (start, end) = self.track_sectors(t.number).unwrap();
        let sector = start as u64 + lba as u64;
        if sector >= end as u64 {
            return err(format!("block {lba} is outside the data track"));
        }
        let base = sector as usize * RAW_SECTOR + off;
        Ok(&self.bin[base..base + USER_DATA])
    }

    fn read_extent(&self, lba: u32, size: usize) -> Result<Vec<u8>, DiscError> {
        let mut out = Vec::with_capacity(size);
        let mut block = lba;
        while out.len() < size {
            let n = (size - out.len()).min(USER_DATA);
            out.extend_from_slice(&self.block(block)?[..n]);
            block += 1;
        }
        Ok(out)
    }

    fn read_filesystem(&mut self) -> Result<(), DiscError> {
        // Volume descriptors start at block 16; type 1 = primary, 255 = terminator.
        let mut root = None;
        let mut volume_id = String::new();
        for lba in 16..64 {
            let b = self.block(lba)?;
            if &b[1..6] != b"CD001" {
                return err(format!("block {lba}: not an ISO9660 volume descriptor"));
            }
            match b[0] {
                1 => {
                    volume_id = String::from_utf8_lossy(&b[40..72]).trim_end().to_string();
                    let block_size = u16::from_le_bytes([b[128], b[129]]);
                    if block_size as usize != USER_DATA {
                        return err(format!("unsupported logical block size {block_size}"));
                    }
                    let rec = &b[156..190];
                    root = Some((le32(rec, 2), le32(rec, 10)));
                    break;
                }
                255 => break,
                _ => {}
            }
        }
        self.volume_id = volume_id;
        let Some((lba, size)) = root else {
            return err("no ISO9660 primary volume descriptor");
        };
        let mut files = Vec::new();
        self.walk(lba, size, "", 0, &mut files)?;
        for (i, f) in files.iter().enumerate() {
            self.by_name.insert(f.path.clone(), i);
        }
        self.files = files;
        Ok(())
    }

    fn walk(
        &self,
        lba: u32,
        size: u32,
        prefix: &str,
        depth: u32,
        out: &mut Vec<FileEntry>,
    ) -> Result<(), DiscError> {
        if depth > 8 {
            return err("directory nesting deeper than ISO9660 allows");
        }
        let dir = self.read_extent(lba, size as usize)?;
        let mut pos = 0;
        while pos < dir.len() {
            let len = dir[pos] as usize;
            if len == 0 {
                // Records never span blocks; a zero length pads to the next block.
                pos = (pos / USER_DATA + 1) * USER_DATA;
                continue;
            }
            if len < 34 || pos + len > dir.len() {
                return err("corrupt directory record");
            }
            let rec = &dir[pos..pos + len];
            pos += len;
            let name_len = rec[32] as usize;
            if 33 + name_len > len {
                return err("corrupt directory record name");
            }
            let name = &rec[33..33 + name_len];
            if name == [0] || name == [1] {
                continue; // "." and ".."
            }
            let mut name = String::from_utf8_lossy(name).to_ascii_uppercase();
            if let Some(i) = name.find(';') {
                name.truncate(i);
            }
            if name.ends_with('.') {
                name.pop();
            }
            let path = format!("{prefix}{name}");
            let (e_lba, e_size, flags) = (le32(rec, 2), le32(rec, 10), rec[25]);
            if flags & 0x02 != 0 {
                self.walk(e_lba, e_size, &format!("{path}/"), depth + 1, out)?;
            } else {
                // Validate the extent now so `read_file` cannot fail later.
                let blocks = e_size.div_ceil(USER_DATA as u32);
                if blocks > 0 {
                    self.block(e_lba + blocks - 1)?;
                }
                out.push(FileEntry { path, lba: e_lba, size: e_size });
            }
        }
        Ok(())
    }
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUE: &str = "FILE \"PUNKYDUCK.bin\" BINARY\nREM SESSION 01\n   TRACK 01 MODE2/2352\n     INDEX 01 00:00:00\n   TRACK 02 AUDIO\n     INDEX 01 03:34:01\n   TRACK 03 AUDIO\n     INDEX 00 03:47:19\n     INDEX 01 03:49:19\n";

    #[test]
    fn cue_parsing() {
        let cue = parse_cue(CUE).unwrap();
        assert_eq!(cue.file, "PUNKYDUCK.bin");
        assert_eq!(cue.tracks.len(), 3);
        assert_eq!(cue.tracks[0].kind, TrackKind::Mode2Raw);
        assert_eq!(cue.tracks[1].start, 16051);
        assert_eq!(cue.tracks[2].index0, Some((3 * 60 + 47) * 75 + 19));
        assert_eq!(cue.tracks[2].start, (3 * 60 + 49) * 75 + 19);
    }

    #[test]
    fn cue_rejects_unknown() {
        assert!(parse_cue("FILE a.bin BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00").is_err());
        assert!(parse_cue("FILE a.wav WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00").is_err());
        assert!(parse_cue("FILE a.bin BINARY\nTRACK 01 AUDIO\nINDEX 01 00:61:00").is_err());
        assert!(parse_cue("TRACK 01 AUDIO\nINDEX 01 00:00:00").is_err());
    }

    #[test]
    fn audio_track_access() {
        // Two audio tracks of 2 and 1 sectors.
        let mut bin = vec![0u8; 3 * RAW_SECTOR];
        bin[2 * RAW_SECTOR..2 * RAW_SECTOR + 4].copy_from_slice(&[0x34, 0x12, 0xff, 0xff]);
        let cue = "FILE x.bin BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:00:02\n";
        let disc = Disc::new(bin, cue).unwrap();
        assert_eq!(disc.audio_track_numbers(), vec![1, 2]);
        assert_eq!(disc.audio_track(1).unwrap().len_frames(), 2 * 588);
        let t = disc.audio_track(2).unwrap();
        assert_eq!(t.len_sectors(), 1);
        assert_eq!(t.frame(0), [0x1234, -1]);
        let mut buf = [0i16; 2400];
        assert_eq!(t.read(0, &mut buf), 588);
        assert_eq!(&buf[..2], &[0x1234, -1]);
        assert_eq!(t.read(587, &mut buf), 1);
        assert_eq!(t.read(600, &mut buf), 0);
    }
}
