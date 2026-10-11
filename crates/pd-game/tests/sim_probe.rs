//! Diagnostic replay of a recorded run (prints a state trace; asserts nothing).
mod sim_common;
use pd_game::settings::Enhancements;
use pd_game::sim::{Event, SimInput};
use sim_common::*;

#[test]
fn trace() {
    let Ok(name) = std::env::var("SIM_PROBE") else { return };
    let first: usize = std::env::var("SIM_FIRST").ok().and_then(|v| v.parse().ok()).unwrap_or(61);
    let Some(demo) = out(&format!("simcmp/{name}/c/PUNKY/DEMOILE1.DAT")) else { return };
    let Some(mut sim) = new_game(1, 1, Enhancements::original()) else { return };
    let recs: Vec<SimInput> = demo.chunks_exact(16).map(|c| SimInput::from_bytes(c.try_into().unwrap())).collect();
    let (mut i, mut steps, mut f) = (0, first, 0);
    let every: usize = std::env::var("SIM_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let from: usize = std::env::var("SIM_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut last_hurt = 0;
    while i + steps <= recs.len() {
        sim.frame(&recs[i], steps as i32 - 1);
        for e in sim.drain_events() {
            match e {
                Event::Sfx { .. } | Event::SeaVolume(_) => {}
                e => eprintln!("frame {f} step {i}: {e:?}"),
            }
        }
        if sim.s.p.hurt > last_hurt {
            eprintln!("frame {f}: hurt set, energy {} bump {} zone {}", sim.s.p.energy, sim.s.p.bump, sim.s.p.zone);
        }
        last_hurt = sim.s.p.hurt;
        if f >= from && f % every == 0 {
            let r = recs[i];
            eprint!("in up{} dn{} l{} r{} f{} | ", r.up as u8, r.down as u8, r.left as u8, r.right as u8, r.fire as u8);
            let p = &sim.s.p;
            eprintln!("frame {f}: pos ({},{}) angle {:.3} energy {} lives {} dead {} zone {} step {} money {}", p.px, p.pz, p.angle, p.energy, p.lives, p.dead, p.zone, sim.s.q.step, p.money);
        }
        i += steps;
        steps = 2;
        f += 1;
    }
    let p = &sim.s.p;
    eprintln!("frame END {f}: pos ({},{}) t ({},{}) angle {} energy {} lives {} dead {} money {} seed {:#x}", p.px, p.pz, p.tx, p.tz, p.angle, p.energy, p.lives, p.dead, p.money, sim.s.rng.seed);
}
