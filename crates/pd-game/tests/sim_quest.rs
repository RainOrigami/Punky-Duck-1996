//! Island quest scripts driven directly: the duck is placed on items and zones and the
//! script's events and state are checked against `docs/re/game/items-and-scripts.md`.
mod sim_common;
use pd_game::settings::Enhancements;
use pd_game::sim::{Event, Sim, SimInput, Status};
use sim_common::*;

/// Put the duck at a world position and run two frames (touch is computed at the end
/// of a frame and used by the script of the next one).
fn goto(sim: &mut Sim, wx: i32, wz: i32) -> Vec<Event> {
    let p = &mut sim.s.p;
    p.px = -wx;
    p.pz = -wz;
    p.tx = -wx;
    p.tz = -wz;
    p.hurt = 0;
    p.energy = 99;
    let mut ev = Vec::new();
    for _ in 0..2 {
        sim.s.p.hurt = 0;
        sim.s.p.energy = 99;
        sim.frame(&SimInput::default(), 1);
        ev.extend(sim.drain_events().into_iter().filter(|e| !matches!(e, Event::Sfx { .. } | Event::SeaVolume(_))));
    }
    ev
}

fn zone_cell(sim: &Sim, zone: u8, skip: impl Fn(i32, i32) -> bool) -> (i32, i32) {
    for x in 2..254 {
        for z in 2..254 {
            if sim.terrain.zone[(x * 256 + z) as usize] == zone && !skip(x, z) {
                return ((x - 128) * 300 + if x >= 128 { 150 } else { -150 }, (z - 128) * 300 + if z >= 128 { 150 } else { -150 });
            }
        }
    }
    panic!("no cell of zone {zone}");
}

#[test]
fn island1_raft_quest() {
    let Some(mut sim) = new_game(1, 1, Enhancements::original()) else {
        eprintln!("disc absent, skipping");
        return;
    };
    // Keep the neighbourhood quiet: no monsters for the walk-through.
    let n = sim.s.nmonsters;
    assert_eq!(n, 147);
    for m in sim.s.monsters.iter_mut() {
        m.ty = 0;
    }
    let site = zone_cell(&sim, 7, |x, z| (0xe1..=0xe3).contains(&x) && (0x46..=0x47).contains(&z));

    // Parts before the plan are refused with line 4 and remembered on the map.
    let ev = goto(&mut sim, -13500, -25500);
    assert!(ev.contains(&Event::Say(4)), "{ev:?}");
    assert_eq!(sim.s.q.busy, 1);
    assert_eq!((sim.s.q.hint_x[0], sim.s.q.hint_z[0]), (-13500, -25500));
    goto(&mut sim, 0, 0);
    assert_eq!(sim.s.q.busy, 0);

    let ev = goto(&mut sim, 18562, 1687);
    assert!(ev.contains(&Event::Picture("RADEAU.RON")));
    assert_eq!((sim.s.q.step, sim.s.q.ninv, sim.s.q.inv[0]), (0, 1, 0));
    assert_eq!(sim.s.items[0].id, -1);

    assert!(goto(&mut sim, -13500, -25500).contains(&Event::Say(0)));
    assert!(goto(&mut sim, 19724, 5864).contains(&Event::Say(1)));
    assert_eq!(sim.s.q.step, 2);
    // Planks need the build site to have been seen.
    assert!(goto(&mut sim, 3000, -600).contains(&Event::Say(5)));
    goto(&mut sim, 0, 0);
    assert!(goto(&mut sim, site.0, site.1).contains(&Event::Say(6)));
    assert_eq!(sim.s.q.flags.site_seen, 1);
    // The trunk is too heavy for now.
    assert!(goto(&mut sim, 21000, 23400).contains(&Event::Say(8)));
    goto(&mut sim, 0, 0);
    assert!(goto(&mut sim, 3000, -600).contains(&Event::Say(3)));
    assert!(goto(&mut sim, -28930, -6818).contains(&Event::Say(2)));
    assert_eq!(sim.s.q.step, 4);
    let ev = goto(&mut sim, site.0, site.1);
    assert!(ev.contains(&Event::Say(10)));
    assert_eq!((sim.s.q.step, sim.s.q.model, sim.s.q.ninv), (5, 2, 3));
    // Raft footprint raised (zone_fill writes z + 1).
    assert_eq!(sim.terrain.zone[0xe1 * 256 + 0x46], 17);
    assert_eq!(sim.terrain.zone[0xe3 * 256 + 0x47], 17);
    assert!(sim.meshes.iter().any(|m| m.slot == 6 && m.model == 2));

    assert!(goto(&mut sim, 21000, 23400).contains(&Event::Say(9)));
    assert!(goto(&mut sim, site.0, site.1).contains(&Event::Say(11)));
    assert!(goto(&mut sim, -13500, 25500).contains(&Event::Say(12)));
    let ev = goto(&mut sim, site.0, site.1);
    assert!(ev.contains(&Event::Say(13)));
    assert_eq!((sim.s.q.step, sim.s.q.model, sim.s.q.ninv), (9, 0, 3));

    // The script asked for the boss: it appears at the map centre in slot 0 on the
    // next frame, and since the duck still stands on the site the "must finish the
    // mission" line follows at once (and only once until the duck leaves).
    assert!(ev.contains(&Event::LoadBossBank) && ev.contains(&Event::Say(14)), "{ev:?}");
    assert_eq!(sim.s.boss_state, 1);
    let b = &sim.s.monsters[0];
    assert_eq!((b.ty, b.hp, b.max_shots, b.fire_chance), (5, 2 * 1 * 20 + 150, 5, 150));
    assert!(!goto(&mut sim, site.0, site.1).contains(&Event::Say(14)));

    // Boss dead: medium difficulty asks for half the monsters (none are alive here).
    sim.s.monsters[0].ty = 0;
    let ev = goto(&mut sim, 0, 0);
    assert_eq!(sim.s.boss_state, 2);
    assert!(ev.contains(&Event::Say(16)) && ev.contains(&Event::Say(15)), "{ev:?}");
    assert_eq!(sim.s.q.step, 11);
    assert_eq!(sim.status(), Status::Playing);
    goto(&mut sim, site.0, site.1);
    assert_eq!(sim.s.next_island, 2);
    assert_eq!(sim.status(), Status::Ending, "0x124ea: island 1 ends this build");
}

#[test]
fn island1_teleporter() {
    let Some(mut sim) = new_game(1, 0, Enhancements::original()) else {
        return;
    };
    for m in sim.s.monsters.iter_mut() {
        m.ty = 0;
    }
    sim.s.boss_state = 9;
    // First contact arms the pad (id 15 -> 16), stuns for 20 frames and marks the map.
    let p = &mut sim.s.p;
    (p.px, p.pz, p.tx, p.tz) = (-31200, -15000, -31200, -15000);
    sim.frame(&SimInput::default(), 1);
    assert_eq!(sim.s.q.touch, 15);
    sim.frame(&SimInput::default(), 1);
    assert_eq!(sim.s.p.hurt, 20);
    assert_eq!(sim.s.items[7].id, 16);
    assert_eq!((sim.s.q.tele_x[0], sim.s.q.tele_z[0]), (31200, 15000));
    let mut frames = 0;
    while sim.s.p.px == -31200 || sim.s.p.px > -31300 && sim.s.p.px < -31000 {
        sim.frame(&SimInput::default(), 1);
        frames += 1;
        assert!(frames < 60, "never teleported, pos {} hurt {}", sim.s.p.px, sim.s.p.hurt);
    }
    assert_eq!((sim.s.p.px, sim.s.p.pz), (20700, 22200));
}

/// Every island starts, runs and saves; the bonus list fills to 1470 with random money.
#[test]
fn all_islands_start_and_run() {
    for n in 1..=5 {
        for enh in [Enhancements::original(), Enhancements::enhanced()] {
            let Some(mut sim) = new_game(n, 2, enh) else {
                return;
            };
            assert!(sim.s.bonus_last >= 1470, "island {n}");
            assert_eq!(sim.s.q.step, -1);
            for f in 0..300 {
                let inp = SimInput { up: f % 3 != 0, left: f % 50 < 10, fire: true, ..Default::default() };
                sim.frame(&inp, 1 + f % 3);
                sim.drain_events();
            }
            let bytes = sim.s.write_save();
            let mut back = sim.s.clone();
            back.read_save(&bytes).unwrap();
            assert!(back.write_save() == bytes);
        }
    }
}
