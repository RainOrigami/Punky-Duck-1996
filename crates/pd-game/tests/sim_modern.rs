//! `Enhancements::modern_controls` on a synthetic flat island (no disc files needed).
use pd_game::settings::Enhancements;
use pd_game::sim::{ModernInput, Sim, SimInput};

fn flat(enh: Enhancements) -> Sim {
    let mut sim = Sim::new(enh);
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

fn modern() -> Enhancements {
    Enhancements { modern_controls: true, ..Enhancements::original() }
}

#[test]
fn mouse_turn_covers_the_requested_angle_in_one_frame() {
    let mut sim = flat(modern());
    let a0 = sim.s.p.angle;
    sim.modern = ModernInput { turn: 0.5, ..Default::default() };
    sim.frame(&SimInput::default(), 4);
    // The game keeps the heading wrapped, so compare modulo a full turn.
    let turned = (sim.s.p.angle - a0).rem_euclid(std::f32::consts::TAU);
    assert!((turned - 0.5).abs() < 1e-3, "turned {turned}");
}

#[test]
fn strafe_keys_match_the_original_modifier_strafe() {
    // A/D without a modifier move exactly like Ctrl + left/right in the original.
    let mut a = flat(modern());
    let mut b = flat(Enhancements::original());
    for _ in 0..20 {
        a.modern = ModernInput { strafe_left: true, ..Default::default() };
        a.frame(&SimInput::default(), 4);
        b.frame(&SimInput { strafe: true, left: true, ..Default::default() }, 4);
    }
    assert_ne!((a.s.p.px, a.s.p.pz), (0, 0));
    assert_eq!((a.s.p.px, a.s.p.pz, a.s.p.angle), (b.s.p.px, b.s.p.pz, b.s.p.angle));
}

#[test]
fn number_keys_select_owned_weapons_only() {
    let mut sim = flat(modern());
    sim.s.p.owned[3] = 1;
    sim.s.p.ammo[3] = 5;
    sim.modern = ModernInput { select_weapon: Some(3), ..Default::default() };
    sim.frame(&SimInput::default(), 4);
    assert_eq!(sim.s.p.weapon, 3);
    sim.modern = ModernInput { select_weapon: Some(5), ..Default::default() };
    sim.frame(&SimInput::default(), 4);
    assert_eq!(sim.s.p.weapon, 3, "weapon 5 is not owned");
}

#[test]
fn original_mode_ignores_modern_input() {
    let mut sim = flat(Enhancements::original());
    let a0 = sim.s.p.angle;
    sim.s.p.owned[3] = 1;
    sim.s.p.ammo[3] = 5;
    sim.modern = ModernInput { turn: 0.5, strafe_left: true, strafe_right: false, select_weapon: Some(3) };
    sim.frame(&SimInput::default(), 4);
    assert_eq!((sim.s.p.angle, sim.s.p.px, sim.s.p.pz, sim.s.p.weapon), (a0, 0, 0, 0));
}
