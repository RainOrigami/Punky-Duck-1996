//! Tests against the original disc image. They need `disc/PUNKYDUCK.{bin,cue}`
//! (and `disc/extracted/PUNKY/` for the comparisons), which are never
//! committed; without them every test passes trivially after printing a note.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pd_engine::assets::DiscAssets;
use pd_engine::audio::CdPlayer;
use pd_engine::{AssetSource, SAMPLE_RATE};
use pd_formats::disc::{Disc, RAW_SECTOR};
use pd_formats::flc::Flc;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../disc")
}

fn disc() -> Option<Disc> {
    let bin = std::fs::read(root().join("PUNKYDUCK.bin")).ok();
    let cue = std::fs::read_to_string(root().join("PUNKYDUCK.cue")).ok();
    match (bin, cue) {
        (Some(bin), Some(cue)) => Some(Disc::new(bin, &cue).expect("disc image parses")),
        _ => {
            eprintln!("SKIPPED: disc/PUNKYDUCK.bin/.cue not present");
            None
        }
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &format!("{prefix}{name}/"), out);
        } else {
            out.insert(format!("{prefix}{name}"), entry.path());
        }
    }
}

#[test]
fn filesystem_matches_extracted_directory() {
    let Some(disc) = disc() else { return };
    assert_eq!(disc.volume_id(), "PUNKY DUCK");
    let extracted_dir = root().join("extracted/PUNKY");
    if !extracted_dir.is_dir() {
        eprintln!("SKIPPED: disc/extracted/PUNKY not present");
        return;
    }
    let mut extracted = BTreeMap::new();
    walk(&extracted_dir, "", &mut extracted);

    // The disc root holds exactly one directory, PUNKY, with all the files.
    assert!(disc.files().iter().all(|f| f.path.starts_with("PUNKY/")));
    let on_disc: BTreeMap<String, u32> =
        disc.files().iter().map(|f| (f.path["PUNKY/".len()..].to_string(), f.size)).collect();
    assert_eq!(
        on_disc.keys().collect::<Vec<_>>(),
        extracted.keys().collect::<Vec<_>>(),
        "file list differs from disc/extracted/PUNKY"
    );
    // Every file byte-identical (31 MB, fast enough), through the AssetSource
    // (which resolves bare names inside PUNKY/).
    let assets = DiscAssets::new(Rc::new(disc));
    assert_eq!(assets.base(), "PUNKY/");
    assert_eq!(assets.names().count(), extracted.len());
    for (name, path) in &extracted {
        let want = std::fs::read(path).unwrap();
        let got = assets.load(name).unwrap();
        assert!(got == want, "{name} differs from the extracted copy");
    }
    // Lookup is case-insensitive and accepts DOS separators.
    assert_eq!(assets.load("p.exe").unwrap().len(), on_disc["P.EXE"] as usize);
    assert_eq!(assets.disc().read_file("punky\\p.exe").unwrap().len(), on_disc["P.EXE"] as usize);
    let sub = extracted.keys().find(|k| k.contains('/')).expect("disc has a sub-directory");
    assert!(assets.load(&sub.to_lowercase().replace('/', "\\")).is_some());
    assert!(assets.load("NOSUCH.FIL").is_none());
}

#[test]
fn audio_tracks_match_cue() {
    let Some(disc) = disc() else { return };
    assert_eq!(disc.audio_track_numbers(), vec![2, 3, 4, 5, 6, 7, 8, 9]);
    let msf = |m: u32, s: u32, f: u32| (m * 60 + s) * 75 + f;
    // INDEX 01 positions from PUNKYDUCK.cue.
    let starts = [
        msf(3, 34, 1),
        msf(3, 49, 19),
        msf(4, 58, 33),
        msf(7, 42, 30),
        msf(9, 49, 67),
        msf(9, 55, 69),
        msf(10, 33, 24),
        msf(12, 17, 28),
    ];
    let total = (std::fs::metadata(root().join("PUNKYDUCK.bin")).unwrap().len() / RAW_SECTOR as u64) as u32;
    assert_eq!(disc.track_sectors(1), Some((0, 16051)));
    for (i, number) in (2u8..=9).enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(total);
        let sectors = (end - starts[i]) as usize;
        let t = disc.audio_track(number).unwrap();
        assert_eq!(t.len_sectors(), sectors, "track {number}");
        assert_eq!(t.len_frames(), sectors * 588, "track {number}");
        assert_eq!(t.bytes().len(), sectors * RAW_SECTOR);
        // Real music, not data misread as audio: not silent, and no sector
        // starts with the CD-ROM sync pattern.
        let v = t.to_vec();
        assert!(v.iter().any(|&s| s.unsigned_abs() > 1000), "track {number} is silent");
        let sync = [0u8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];
        assert!(!t.bytes().chunks_exact(RAW_SECTOR).any(|s| s[..12] == sync), "track {number} holds data sectors");
        println!(
            "track {number}: {sectors} sectors = {:.2} s",
            t.len_frames() as f64 / SAMPLE_RATE as f64
        );
    }
    assert!(disc.audio_track(1).is_none());
    assert!(disc.audio_track(10).is_none());
}

#[test]
fn cd_player_plays_and_loops() {
    let Some(disc) = disc() else { return };
    let disc = Rc::new(disc);
    let mut cd = CdPlayer::new(Some(disc.clone()));
    let track = disc.audio_track(6).unwrap(); // shortest track
    let len = track.len_frames();

    assert!(cd.play(6, false));
    let mut out = vec![0i16; (len + 100) * 2];
    cd.mix_into(&mut out);
    assert!(!cd.is_playing());
    assert!(out[..len * 2] == track.to_vec()[..]);
    assert!(out[len * 2..].iter().all(|&s| s == 0));

    assert!(cd.play(6, true));
    out.fill(0);
    cd.mix_into(&mut out);
    assert!(cd.is_playing());
    assert_eq!(cd.position_frames(), 100);
    assert!(out[len * 2..] == track.to_vec()[..200]);

    assert!(!cd.play(1, true), "the data track is not playable");
}

#[test]
fn all_movies_decode() {
    let Some(disc) = disc() else { return };
    let assets = DiscAssets::new(Rc::new(disc));
    // (name, frames, ms per frame) as read from the headers.
    let movies = [
        ("C_SOFT.MOV", 101, 100),
        ("INTRO.MOV", 560, 114),
        ("PART_1.MOV", 12, 114),
        ("PART_2.MOV", 71, 114),
        ("PUNKY.MOV", 20, 71),
    ];
    let on_disc: Vec<String> =
        assets.disc().files().iter().filter(|f| f.path.ends_with(".MOV")).map(|f| f.path.clone()).collect();
    assert_eq!(on_disc.len(), movies.len(), "unexpected set of .MOV files: {on_disc:?}");

    for (name, frames, speed) in movies {
        let data = assets.load(name).unwrap_or_else(|| panic!("{name} missing"));
        let mut flc = Flc::new(data).unwrap();
        let h = *flc.header();
        assert_eq!((h.width, h.height, h.depth), (640, 480, 8), "{name}");
        assert_eq!((h.frames, h.speed_ms), (frames, speed), "{name}");
        let mut n = 0;
        let mut total_ms = 0u64;
        while let Some(info) = flc.decode_next().unwrap_or_else(|e| panic!("{name} frame {n}: {e}")) {
            assert_eq!(info.index, n);
            if n == 0 {
                assert!(info.palette_changed && info.pixels_changed, "{name}: first frame is a key frame");
            }
            total_ms += info.delay_ms as u64;
            n += 1;
        }
        assert_eq!(n, frames as u32, "{name}");
        // A decoded movie ends on a real picture: several colours, non-black palette.
        let mut used = [false; 256];
        flc.pixels().iter().for_each(|&p| used[p as usize] = true);
        assert!(used.iter().filter(|&&u| u).count() > 1 || name == "C_SOFT.MOV", "{name}: last frame is flat");
        assert!(flc.palette().iter().any(|c| *c != [0, 0, 0]), "{name}: palette is black");
        println!("{name}: {n} frames, {:.1} s", total_ms as f64 / 1000.0);
    }
}
