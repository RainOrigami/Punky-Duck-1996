//! Shared helpers for the `sim_*` integration tests. Disc files and reference saves are
//! optional: tests skip cleanly when they are absent.
#![allow(dead_code)]

use pd_formats::trj::Trj;
use pd_formats::uni::Uni;
use pd_game::settings::Enhancements;
use pd_game::sim::Sim;

pub fn root() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string()
}

pub fn disc(name: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("{}/disc/extracted/PUNKY/{name}", root())).ok()
}

pub fn out(name: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("{}/out/{name}", root())).ok()
}

pub struct Island {
    pub uni: Uni,
    pub bonus: Trj,
    pub monsters: Trj,
}

pub fn island(n: i32) -> Option<Island> {
    Some(Island {
        uni: Uni::parse(&disc(&format!("ILE_{n}.UNI"))?).ok()?,
        bonus: Trj::parse(&disc(&format!("ILE0{n}.TRJ"))?).ok()?,
        monsters: Trj::parse(&disc(&format!("MONSTER{n}.TRJ"))?).ok()?,
    })
}

/// A new game on island `n` as the original starts it from a fresh program.
pub fn new_game(n: i32, difficulty: i32, enh: Enhancements) -> Option<Sim> {
    let isl = island(n)?;
    let mut sim = Sim::new(enh);
    sim.s.difficulty = difficulty;
    sim.new_game();
    sim.start_island(n, &isl.uni, &isl.bonus, isl.monsters);
    sim.enter();
    Some(sim)
}
