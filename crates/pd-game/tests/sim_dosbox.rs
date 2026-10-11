//! Whole-state comparison against the original running in DOSBox-X.
//!
//! `out/simcmp/run.sh NAME ...` (recipe in `docs/re/reference.md`) runs a data-patched
//! copy of the original that records its 16-byte input record for every simulation step
//! into `DEMOILE1.DAT` and, with game speed -6, runs exactly two steps per rendered frame
//! after the first one. The run ends with a save game. Here the same step inputs are fed
//! to the simulation from a fresh program state and the resulting save image is compared
//! with the original's, byte for byte.
//!
//! Skips when the run directory is absent.
mod sim_common;
use pd_game::settings::Enhancements;
use pd_game::sim::{Event, Sim, SimInput, State};
use sim_common::*;

struct Diff {
    bytes: usize,
    regions: Vec<(&'static str, usize)>,
}

const REGIONS: &[(&str, usize, usize)] = &[
    ("header", 0, 32),
    ("inventory", 32, 112),
    ("position", 112, 144),
    ("shot types", 144, 344),
    ("weapons", 344, 428),
    ("player misc", 428, 516),
    ("bonus frames", 516, 1316),
    ("bonus list", 1316, 33316),
    ("items", 33316, 34036),
    ("compass/boss", 34036, 34044),
    ("markers", 34044, 34524),
    ("explored", 34524, 100060),
    ("flags", 100060, 100108),
    ("monsters", 100108, 181588),
    ("sprite tables", 181588, 181813),
    ("difficulty", 181813, 181817),
];

fn diff(a: &[u8], b: &[u8]) -> Diff {
    let mut regions = Vec::new();
    let mut bytes = 0;
    for &(name, lo, hi) in REGIONS {
        let n = (lo..hi).filter(|&i| a[i] != b[i]).count();
        if n > 0 {
            regions.push((name, n));
        }
        bytes += n;
    }
    Diff { bytes, regions }
}

/// Replay `demo` (one record per step). Every frame is two steps except the first one
/// and the one after each respawn: both follow a palette fade, during which the 70 Hz
/// counter keeps running, so they run `first_steps` / `fade_steps` steps.
fn replay(demo: &[u8], first_steps: usize, fade_steps: usize, difficulty: i32, load: Option<&[u8]>) -> Option<(Sim, bool, bool)> {
    let mut sim = match load {
        None => new_game(1, difficulty, Enhancements::original())?,
        Some(save) => {
            // LOAD GAME from a freshly started program.
            let isl = island(1)?;
            let mut sim = Sim::new(Enhancements::original());
            sim.s.difficulty = difficulty;
            sim.load_save(save, &isl.uni, isl.monsters).ok()?;
            sim.enter();
            sim
        }
    };
    let recs: Vec<SimInput> =
        demo.chunks_exact(16).map(|c| SimInput::from_bytes(c.try_into().unwrap())).collect();
    let mut i = 0;
    let mut steps = first_steps;
    let mut faded = false;
    while i + steps <= recs.len() {
        sim.frame(&recs[i], steps as i32 - 1);
        i += steps;
        steps = 2;
        if sim.drain_events().contains(&Event::FadeIn) {
            steps = fade_steps;
            faded = true;
        }
    }
    Some((sim, faded, i == recs.len()))
}

fn compare(name: &str, load: Option<&str>) -> Option<((usize, usize), Diff, State, State)> {
    let demo = out(&format!("simcmp/{name}/c/PUNKY/DEMOILE1.DAT"))?;
    let save = out(&format!("simcmp/{name}/c/PUNKY/SAVEGAME.__0"))?;
    let stp = out(&format!("simcmp/{name}/c/PUNKY/GAME.STP"))?;
    let difficulty = i32::from_le_bytes(stp[20..24].try_into().unwrap());
    let mut orig = State::default();
    orig.read_save(&save).ok()?;
    let loaded = match load {
        Some(f) => Some(out(f)?),
        None => None,
    };
    let mut best: Option<((usize, usize), Diff, State)> = None;
    // The lengths of the post-fade frames are the only unknowns.
    let order: Vec<usize> = (0..50usize).flat_map(|d| [61 + d, 60usize.saturating_sub(d)]).filter(|k| *k >= 2).collect();
    'outer: for first in order {
        for fade in 2..80usize {
            let (sim, faded, complete) = replay(&demo, first, fade, difficulty, loaded.as_deref())?;
            if complete {
                let d = diff(&sim.s.write_save(), &save);
                if best.as_ref().map_or(true, |b| d.bytes < b.1.bytes) {
                    best = Some(((first, fade), d, sim.s.clone()));
                }
                if best.as_ref().unwrap().1.bytes == 0 {
                    break 'outer;
                }
            }
            if !faded {
                break;
            }
        }
    }
    let (first, d, s) = best?;
    Some((first, d, s, orig))
}

fn report(name: &str, load: Option<&str>) -> Option<usize> {
    let (first, d, s, o) = compare(name, load)?;
    eprintln!("[{name}] best (first-frame, post-respawn-frame) steps = {first:?}; differing save bytes = {} of 181817", d.bytes);
    for (r, n) in &d.regions {
        eprintln!("[{name}]   {r}: {n} bytes differ");
    }
    eprintln!(
        "[{name}] sim  px,pz=({},{}) tx,tz=({},{}) angle={} energy={} lives={} dead={}/{} hurt={} money={}",
        s.p.px, s.p.pz, s.p.tx, s.p.tz, s.p.angle, s.p.energy, s.p.lives, s.p.dead, s.p.dead_wait, s.p.hurt, s.p.money
    );
    eprintln!(
        "[{name}] orig px,pz=({},{}) tx,tz=({},{}) angle={} energy={} lives={} dead={}/{} hurt={} money={}",
        o.p.px, o.p.pz, o.p.tx, o.p.tz, o.p.angle, o.p.energy, o.p.lives, o.p.dead, o.p.dead_wait, o.p.hurt, o.p.money
    );
    let mdiff: Vec<usize> = (0..210).filter(|&i| s.monsters[i] != o.monsters[i]).collect();
    eprintln!("[{name}] monster slots differing: {:?}", mdiff);
    for &i in &mdiff {
        let a = format!("{:?}", s.monsters[i]);
        let b = format!("{:?}", o.monsters[i]);
        let d: Vec<String> = a.split(", ").zip(b.split(", ")).filter(|(x, y)| x != y).map(|(x, y)| format!("{x} / {y}")).collect();
        eprintln!("[{name}]  m{i} ty {} ai {} sim/orig: {}", o.monsters[i].ty, o.monsters[i].ai, d.join("; "));
    }
    let bd: Vec<usize> = (0..1600).filter(|&i| s.bonus[i] != o.bonus[i]).collect();
    eprintln!("[{name}] bonus entries differing: {} {:?}", bd.len(), &bd[..bd.len().min(8)]);
    Some(d.bytes)
}

macro_rules! run {
    ($fn:ident, $name:literal) => {
        run!($fn, $name, None);
    };
    ($fn:ident, $name:literal, $load:expr) => {
        #[test]
        fn $fn() {
            match report($name, $load) {
                None => eprintln!("out/simcmp/{} absent, skipping", $name),
                Some(n) => assert_eq!(n, 0, "save image differs from the original"),
            }
        }
    };
}

run!(walk_west_and_die, "t1");
run!(turn_walk_fire, "t2");
run!(bisect_2, "b2");
run!(bisect_3, "b3");
run!(bisect_4, "b4");
// LOAD GAME of an edited save: the duck 5400 units from path monster 81 (inside the
// 6000-unit activity box, outside its detect radius), 14 s without moving: idle path
// following exactly as coded (open-questions Q7).
run!(idle_path_monsters_after_load, "q7", Some("simcmp/q7_in.sav"));
