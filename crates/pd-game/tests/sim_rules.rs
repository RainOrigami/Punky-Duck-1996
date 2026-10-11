//! Rules from `docs/re/game/*.md` on a synthetic flat island (no disc files needed).
use pd_game::settings::Enhancements;
use pd_game::sim::{Event, Sim, SimInput, Status};

fn flat() -> Sim {
    let mut sim = Sim::new(Enhancements::original());
    sim.new_game();
    sim.s.island = 1;
    sim.s.q.step = -1;
    sim.s.q.step_shown = -1;
    sim.s.q.touch = -1;
    sim.s.q.model = -1;
    sim.s.bonus_last = -1;
    sim.s.item_last = -1;
    sim.enter();
    sim
}

fn up() -> SimInput {
    SimInput { up: true, ..Default::default() }
}

#[test]
fn new_game_values() {
    let sim = flat();
    let p = &sim.s.p;
    assert_eq!((p.lives, p.energy, p.money, p.continues, p.fuel), (3, 99, 0, 0, 0));
    assert_eq!(p.ammo[0], 10_000_000);
    assert_eq!(p.owned, [1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(p.slide, 8);
    assert_eq!(p.walk, 1_000_000_000);
}

#[test]
fn walking_is_target_plus_chase() {
    let mut sim = flat();
    // angle 0: forward is -x in stored coordinates; target moves 34 per step, the duck
    // covers 1/8 of the remaining distance (truncating).
    sim.s.p.ticks = 1;
    sim.step(&up());
    assert_eq!((sim.s.p.tx, sim.s.p.px), (-34, -4));
    sim.step(&up());
    assert_eq!((sim.s.p.tx, sim.s.p.px), (-68, -4 - 8));
    for _ in 0..200 {
        sim.step(&up());
    }
    let x0 = sim.s.p.px;
    sim.step(&up());
    assert_eq!(x0 - sim.s.p.px, 34, "steady state speed");
    assert_eq!(sim.s.p.pz, 0);
    // Releasing the key: the duck stops less than `slide` units short of the target.
    for _ in 0..200 {
        sim.step(&SimInput::default());
    }
    assert!((sim.s.p.px - sim.s.p.tx).abs() < 8);
}

#[test]
fn turn_rate_is_per_frame() {
    // 0.07 / ticks per step, ticks + 1 steps per frame.
    for ticks in [1, 2, 4] {
        let mut sim = flat();
        sim.frame(&SimInput { left: true, ..Default::default() }, ticks);
        let want = -0.07 * (ticks as f64 + 1.0) / ticks as f64;
        assert!((sim.s.p.angle as f64 - want).abs() < 1e-6, "ticks {ticks}");
    }
    // monsters_update keeps the angle <= 0.
    let mut sim = flat();
    sim.frame(&SimInput { right: true, ..Default::default() }, 1);
    assert!((sim.s.p.angle as f64 - (0.14 - 6.2831854)).abs() < 1e-5);
}

#[test]
fn zones_block_and_slow() {
    let mut sim = flat();
    sim.terrain.zone.iter_mut().for_each(|z| *z = 8);
    sim.s.p.ticks = 1;
    sim.step(&up());
    assert_eq!((sim.s.p.px, sim.s.p.tx), (0, 0), "solid zone: target snaps back");
    assert_eq!(sim.s.p.zone, 8);
    let mut sim = flat();
    sim.terrain.zone.iter_mut().for_each(|z| *z = 1);
    sim.s.p.ticks = 1;
    sim.step(&up());
    assert_eq!(sim.s.p.tx, -(34 / 5));
    let mut sim = flat();
    sim.terrain.zone.iter_mut().for_each(|z| *z = 6);
    sim.s.p.ticks = 1;
    sim.step(&SimInput::default());
    assert_eq!((sim.s.p.energy, sim.s.p.hurt), (94, 20));
}

#[test]
fn drowning_every_11_steps() {
    let mut sim = flat();
    sim.terrain.zone.iter_mut().for_each(|z| *z = 5);
    sim.s.p.ticks = 1;
    for _ in 0..10 {
        sim.step(&SimInput::default());
    }
    assert_eq!(sim.s.p.energy, 99);
    sim.step(&SimInput::default());
    assert_eq!((sim.s.p.energy, sim.s.p.hurt, sim.s.p.drown), (89, 20, 0));
    // Swim timer suspends it.
    let mut sim = flat();
    sim.terrain.zone.iter_mut().for_each(|z| *z = 5);
    sim.s.p.swim_timer = 300;
    for _ in 0..50 {
        sim.step(&SimInput::default());
    }
    assert_eq!((sim.s.p.energy, sim.s.p.swim_timer), (99, 250));
}

#[test]
fn hurt_and_death_timeline() {
    let mut sim = flat();
    sim.s.p.hurt = 20;
    let idle = SimInput::default();
    sim.frame(&idle, 1);
    assert_eq!(sim.s.p.hurt, 19);
    sim.drain_events();
    sim.frame(&idle, 1);
    assert!(sim.drain_events().contains(&Event::Sfx { sample: 2, volume: 255 }), "quack at 19");
    // Death: 33 falling frames, 40 lying, respawn with one life less.
    let mut sim = flat();
    sim.s.p.start_x = 1234;
    sim.s.p.start_z = -77;
    sim.s.p.energy = 0;
    sim.frame(&idle, 1);
    assert_eq!(sim.s.p.dead, 35);
    assert!(sim.drain_events().contains(&Event::CdTrack(6)));
    let mut frames = 0;
    while sim.s.p.dead != 0 {
        sim.frame(&up(), 1);
        frames += 1;
        assert!(frames < 200);
    }
    assert_eq!(frames, 33 + 40);
    assert_eq!((sim.s.p.lives, sim.s.p.energy, sim.s.p.px, sim.s.p.pz), (2, 99, 1234, -77));
    assert!(sim.drain_events().contains(&Event::CdTrack(5)));
}

#[test]
fn hud_takes_a_second_life_on_a_big_hit() {
    let run = |enh: Enhancements| {
        let mut sim = flat();
        sim.enh = enh;
        sim.s.p.energy = 2;
        sim.s.hud_energy = 2;
        // A hit that leaves energy negative for the HUD of that frame.
        sim.frame_begin();
        sim.s.p.ticks = 1;
        sim.step(&SimInput::default());
        sim.s.p.energy = -3;
        sim.frame_end(&SimInput::default());
        let mut n = 0;
        while sim.s.p.dead != 0 && n < 200 {
            sim.frame(&SimInput::default(), 1);
            n += 1;
        }
        sim.s.p.lives
    };
    assert_eq!(run(Enhancements::original()), 1);
    let mut e = Enhancements::original();
    e.fix_hud_double_life_loss = true;
    assert_eq!(run(e), 2);
}

#[test]
fn jetpack_fuel() {
    let mut sim = flat();
    sim.s.p.fuel = 5000;
    sim.frame(&SimInput { jetpack: true, ..Default::default() }, 1);
    assert_eq!(sim.s.p.flying, 1);
    assert_eq!(sim.s.p.fuel, 4999, "HUD caps the tank");
    let mut frames = 0;
    while sim.s.p.flying == 1 {
        sim.frame(&up(), 1);
        frames += 1;
    }
    assert_eq!(frames, 1000);
    assert_eq!(sim.s.p.fuel, 0);
}

#[test]
fn firing_and_cooldown() {
    let mut sim = flat();
    let fire = SimInput { fire: true, ..Default::default() };
    sim.s.p.ticks = 1;
    let mut shots = Vec::new();
    for step in 0..100 {
        let before = sim.s.shots.next;
        sim.step(&fire);
        if sim.s.shots.next != before {
            shots.push(step);
        }
    }
    // The first step sees the weapon change (last_weapon starts at -1) and zeroes the
    // counter; it must then exceed 25, and every shot zeroes it again.
    assert_eq!(shots, vec![26, 52, 78]);
    assert_eq!(sim.s.p.ammo[0], 10_000_000 - 3);
    // Shot speed 60..79 along the facing, height py + 9000.
    assert!((60..80).contains(&sim.s.shots.vx[0]));
    assert_eq!(sim.s.shots.vz[0], 0);
    assert_eq!(sim.s.shots.y[0], 9000);
    // Flat ground at 0: shots time out silently after 99 steps.
    for _ in 0..200 {
        sim.step(&SimInput::default());
    }
    assert!(sim.s.shots.state.iter().all(|&s| s == 0));
}

#[test]
fn spent_explosions_stay_at_minus_one() {
    for fixed in [false, true] {
        let mut sim = flat();
        sim.enh.fix_spent_shot_slots = fixed;
        sim.s.shots.state[3] = 200;
        for _ in 0..10 {
            sim.frame(&SimInput::default(), 1);
        }
        assert_eq!(sim.s.shots.state[3], if fixed { 0 } else { -1 });
    }
}

#[test]
fn weapon_cycle_and_power() {
    let mut sim = flat();
    sim.s.p.owned[3] = 1;
    sim.s.p.ammo[3] = 5;
    sim.s.p.owned[5] = 1; // no ammo: skipped
    let next = SimInput { next_weapon: true, ..Default::default() };
    sim.frame(&next, 1);
    assert_eq!(sim.s.p.weapon, 3);
    sim.frame(&next, 1);
    assert_eq!(sim.s.p.weapon, 0);
}

#[test]
fn island1_exit_is_rewritten_to_the_ending() {
    let mut sim = flat();
    sim.s.next_island = 2;
    assert_eq!(sim.status(), Status::Ending);
    sim.enh.island1_leads_to_island2 = true;
    assert_eq!(sim.status(), Status::NextIsland(2));
    sim.s.next_island = 0;
    sim.s.p.lives = -1;
    assert_eq!(sim.status(), Status::GameOver);
}

#[test]
fn input_record_round_trip() {
    let mut b = [0u8; 16];
    b[0] = 1;
    b[9] = 1;
    b[12] = 1;
    let i = SimInput::from_bytes(&b);
    assert!(i.up && i.fire && i.esc && !i.down);
    assert_eq!(i.to_bytes(), b);
}
