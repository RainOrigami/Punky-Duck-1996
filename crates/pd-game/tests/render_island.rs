//! Renders island 1 scenes and compares the back buffer with stored hashes.
//!
//! The scenes are the ones checked pixel by pixel against DOSBox-X captures of
//! the original (see `docs/re/engine/renderer.md`, "Verification"); the hashes
//! pin that output. Skips when the disc files are not present.

use pd_formats::bnk::Bnk;
use pd_game::render::{
    Anchor, Camera, IslandAssets, ObjectInstance, Renderer, Scene, SpriteBank, ViewMode, ViewState,
    WorldSprite,
};
use std::path::PathBuf;

struct Disc {
    island: IslandAssets,
    duck: SpriteBank,
}

fn disc() -> Option<Disc> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../disc/extracted/PUNKY");
    let read = |n: &str| std::fs::read(dir.join(n)).ok();
    let (Some(uni), Some(col), Some(m3d), Some(map), Some(pic), Some(banque)) = (
        read("ILE_1.UNI"),
        read("ILE_1.COL"),
        read("OBJETS_1.3D"),
        read("OBJETS_1.MAP"),
        read("PICTURE.BNK"),
        read("BANQUE.BNK"),
    ) else {
        eprintln!("disc files not found in {}, skipping", dir.display());
        return None;
    };
    let pic = Bnk::parse(&pic).unwrap();
    let banque = Bnk::parse(&banque).unwrap();
    Some(Disc {
        island: IslandAssets::from_bytes(&uni, &col, Some((&m3d, &map)), pic.get("CIEL_1.RON")).unwrap(),
        duck: SpriteBank::parse(banque.get("DUCK.SPT").unwrap()).unwrap(),
    })
}

fn fnv(data: impl Iterator<Item = u8>) -> u64 {
    data.fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Island 1 fixed scenery (`0x187e0`).
fn meshes() -> Vec<ObjectInstance> {
    let empty = ObjectInstance { active: false, model: 0, x: 0.0, y: 0.0, z: 0.0, rot_a: 0.0, rot_b: 0.0 };
    let mut v = vec![empty; 8];
    for (slot, model, x, z) in [
        (0, 4, 15881.0, 3225.0),
        (1, 5, 18318.0, 2700.0),
        (2, 4, 18318.0, 4481.0),
        (3, 5, 20662.0, 2981.0),
        (5, 6, -11868.0, -25050.0),
        (7, 7, -31600.0, 26850.0),
    ] {
        v[slot] = ObjectInstance { active: true, model, x, z, ..empty };
    }
    v
}

/// Camera at rest at stored position (`px`, `pz`), duck idle, horizon marker
/// queued as `player_draw` does; two frames so the sky uses the found horizon.
fn render(d: &Disc, px: i32, py: i32, pz: i32, angle: f32, mode: ViewMode, phase: i32) -> (u64, i32, u32) {
    let (c, s) = ((angle as f64).cos(), (angle as f64).sin());
    let sprite = |index, x, y, z, scale| WorldSprite {
        bank: &d.duck,
        index,
        flags: 0,
        x,
        y,
        z,
        angle: 0.0,
        scale,
        anchor: Anchor::HotSpot,
    };
    let sprites = [
        sprite(0x2d, (c * 3000.0 + (-px) as f64) as i32, 0, (s * 3000.0 + (-pz) as f64) as i32, -999.0),
        sprite(0x38, -px, py, -pz, -1.0),
    ];
    let objects = meshes();
    let mut view = ViewState::default();
    view.settle(py);
    view.wave_phase = phase;
    let mut r = Renderer::new();
    let mut overruns = 0;
    for _ in 0..2 {
        let info = r.render(&Scene {
            island: &d.island,
            mode,
            interlace: false,
            camera: Camera { x: px, z: pz, heading: angle },
            view,
            objects: &objects,
            sprites: &sprites,
        });
        view.horizon = info.horizon;
        overruns = info.fog_overruns;
    }
    let (w, h) = mode.size();
    let rows = r.back().chunks_exact(640).take(h as usize);
    (fnv(rows.flat_map(|row| row[..w as usize].iter().copied())), view.horizon, overruns)
}

#[test]
fn island1_views_match_stored_hashes() {
    let Some(d) = disc() else { return };
    // (name, px, py, pz, angle, mode, water phase, hash, horizon)
    let cases = [
        ("start", -19650, 27600, -10575, 0.0f32, ViewMode::High, 0, 0xab7e207a222c3c2cu64, 171),
        ("start turned", -19650, 27600, -10575, -0.9, ViewMode::High, 0, 0x68a0bda026778c61, 170),
        ("huts", -16500, 0, -3600, 0.0, ViewMode::High, 0, 0xfaa68dbb07e8c601, 126),
        ("huts 320", -16500, 0, -3600, 0.0, ViewMode::LowWindow, 0, 0x470c2cd89b4fb94c, 43),
        ("water", -10350, 15300, 21150, 0.0, ViewMode::High, 64, 0x15bbbccbb195ca05, 151),
    ];
    let mut failed = false;
    for (name, px, py, pz, angle, mode, phase, hash, horizon) in cases {
        let (got, hz, overruns) = render(&d, px, py, pz, angle, mode, phase);
        if got != hash || hz != horizon {
            eprintln!("{name}: hash {got:#018x} horizon {hz}, expected {hash:#018x} {horizon}");
            failed = true;
        }
        assert_eq!(overruns, 0, "{name}: fog level left 0..=31");
    }
    assert!(!failed, "render output changed");
}

#[test]
fn view_state_settles_like_the_original() {
    // eye/look heights chase altitude/300 + 250 / + 100 by fifths and stop up to 4 short.
    let mut v = ViewState::default();
    v.settle(27600);
    assert_eq!((v.eye_h, v.look_h), (338, 188));
    let mut v = ViewState::default();
    v.begin_frame(27600);
    assert_eq!((v.eye_h, v.look_h, v.wave_phase), (50 + (342 - 50) / 5, 192 / 5, 16));
}

#[test]
fn empty_scene_renders_without_assets() {
    // No objects, no sprites, no sky picture: must not panic for any heading or mode.
    let Some(d) = disc() else { return };
    let mut r = Renderer::new();
    for (i, mode) in [ViewMode::Low13h, ViewMode::LowWindow, ViewMode::High].into_iter().enumerate() {
        for k in 0..16 {
            let info = r.render(&Scene {
                island: &d.island,
                mode,
                interlace: k & 1 == 1,
                camera: Camera { x: 37000 - k * 5000, z: -38000 + k * 4900, heading: -(k as f32) * 0.4 },
                view: ViewState { eye_h: 250, look_h: 100, wave_phase: k * 16, horizon: 100 + i as i32 * 50 },
                objects: &[],
                sprites: &[],
            });
            assert!(info.horizon >= 100);
        }
    }
}
