//! Save-game layout against a save written by the original in DOSBox-X.
mod sim_common;
use pd_game::settings::Enhancements;
use pd_game::sim::{Sim, State};
use sim_common::*;

#[test]
fn original_save_round_trips_byte_identically() {
    let Some(data) = out("ref/island1_start_SAVEGAME.__0") else {
        eprintln!("reference save absent, skipping");
        return;
    };
    let mut s = State::default();
    s.read_save(&data).unwrap();
    assert_eq!(s.island, 1);
    assert_eq!(s.q.step, -1);
    assert_eq!((s.p.px, s.p.pz), (-19650, -10575));
    assert_eq!((s.p.start_x, s.p.start_z), (-19650, -10575));
    assert_eq!(s.p.lives, 3);
    assert_eq!(s.bonus_last, 1471);
    assert_eq!(s.item_last, 8);
    assert_eq!(s.difficulty, 1);
    assert_eq!(s.explored.iter().filter(|&&b| b != 0).count(), 900);
    let back = s.write_save();
    assert_eq!(back.len(), data.len());
    assert!(back == data, "first difference at {:?}", back.iter().zip(&data).position(|(a, b)| a != b));
}

/// Start island 1 ourselves (fresh program, medium difficulty) and compare with what
/// the original had in memory: this checks the Watcom rand port and its seed (random
/// money cells), the bonus loader, quest items, markers, monster spawn arithmetic,
/// difficulty formulas, sprite tables and the stale TRJ pointers.
#[test]
fn island1_start_matches_original_save() {
    let (Some(data), Some(isl)) = (out("ref/island1_start_SAVEGAME.__0"), island(1)) else {
        eprintln!("reference save or disc absent, skipping");
        return;
    };
    let mut orig = State::default();
    orig.read_save(&data).unwrap();
    let mut sim = Sim::new(Enhancements::original());
    sim.s.difficulty = 1;
    sim.new_game();
    sim.start_island(1, &isl.uni, &isl.bonus, isl.monsters);
    sim.enter();
    let s = &sim.s;

    assert_eq!(s.bonus_last, orig.bonus_last);
    for i in 0..1600 {
        let (a, b) = (s.bonus[i], orig.bonus[i]);
        assert_eq!((a.ty, a.x, a.y, a.z), (b.ty, b.x, b.y, b.z), "bonus {i}");
    }
    assert_eq!(s.items, orig.items);
    assert_eq!(s.item_last, orig.item_last);
    assert_eq!(s.q.tele_x, orig.q.tele_x);
    assert_eq!(s.q.hint_z, orig.q.hint_z);
    assert_eq!(s.bonus_frames, orig.bonus_frames);
    assert_eq!(s.sprite_tables, orig.sprite_tables);
    assert_eq!(s.class_bytes, orig.class_bytes);
    assert_eq!(s.p.ammo, orig.p.ammo);
    assert_eq!(s.p.owned, orig.p.owned);
    assert_eq!((s.p.start_x, s.p.start_z, s.p.lives), (orig.p.start_x, orig.p.start_z, orig.p.lives));

    // Monsters the 20 s reference run never activated are still exactly as spawned.
    let mut same = 0;
    for i in 0..210 {
        let (a, b) = (&s.monsters[i], &orig.monsters[i]);
        assert_eq!(a.ty, b.ty, "monster {i} type");
        assert_eq!((a.path_ptr, a.rec_ptr), (b.path_ptr, b.rec_ptr), "monster {i} pointers");
        assert_eq!((a.detect, a.attack, a.leash, a.max_shots, a.fire_chance, a.shot_speed, a.spread),
                   (b.detect, b.attack, b.leash, b.max_shots, b.fire_chance, b.shot_speed, b.spread), "monster {i}");
        assert_eq!((a.seg_x0, a.seg_z0, a.rot_cos.to_bits(), a.rot_sin.to_bits()),
                   (b.seg_x0, b.seg_z0, b.rot_cos.to_bits(), b.rot_sin.to_bits()), "monster {i} path");
        if a == b {
            same += 1;
        }
    }
    eprintln!("{same} of 210 monster slots identical to the original save");
    assert!(same >= 200, "only {same} monster slots identical");
}
