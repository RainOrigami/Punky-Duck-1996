//! `pd-extract scene`: renders one island view frame headlessly with
//! `pd_game::render` and writes it as an indexed PNG (640x480, the view placed
//! as the game presents it). Used to compare the port with DOSBox-X captures.
//!
//! ```text
//! pd-extract scene <disc dir> <out.png> [--island N] [--save SAVEGAME.__n]
//!     [--px X --pz Z] [--py Y] [--angle RAD] [--mode 0|1|2] [--interlace]
//!     [--duck FRAME|none] [--meshes] [--bonus] [--phase N] [--frames N] [--full]
//! ```
//!
//! `--px/--pz` are the stored (negated) player coordinates as in the save
//! game. Without `--py` the height comes from the terrain. `--frames N` runs
//! the camera smoothing N times from the start-up values instead of letting it
//! settle. `--meshes` queues island 1's fixed scenery meshes. `--bonus` (with
//! `--save`) queues the pickup/decor sprites of the save's bonus list the way
//! `pickups_update` does, animated kinds at their first frame. `--ground`
//! (after `--save`) replaces the saved height by the terrain height.

use pd_engine::Framebuffer;
use pd_formats::bnk::Bnk;
use pd_game::render::{
    Anchor, Camera, IslandAssets, ObjectInstance, Renderer, Scene, SpriteBank, ViewMode, ViewState,
    WorldSprite,
};
use pd_game::settings::Enhancements;
use std::error::Error;
use std::fs;
use std::io::BufWriter;
use std::path::Path;

type Res<T> = Result<T, Box<dyn Error>>;

fn read(dir: &Path, name: &str) -> Res<Vec<u8>> {
    fs::read(dir.join(name))
        .or_else(|_| fs::read(dir.join(name.to_lowercase())))
        .map_err(|e| format!("{name}: {e}").into())
}

/// `terrain_height` (`0x13010`) on the UNI height bytes.
fn terrain_height(island: &IslandAssets, x: i32, z: i32) -> i32 {
    let h = |a: i32, b: i32| -> i32 {
        if (0..256).contains(&a) && (0..256).contains(&b) {
            island.uni.heights[(a * 256 + b) as usize] as i32 * 3
        } else {
            0
        }
    };
    let (x, z) = (x + 38400, z + 38400);
    let (xc, xf, zc, zf) = (x / 300, x % 300, z / 300, z % 300);
    let a = h(xc, zc) + (h(xc + 1, zc) - h(xc, zc)) * xf / 300;
    let b = h(xc, zc + 1) + (h(xc + 1, zc + 1) - h(xc, zc + 1)) * xf / 300;
    ((b - a) * zf / 300 + a) * 300
}

pub fn run(args: &[String]) -> Res<()> {
    let (Some(dir), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: pd-extract scene <disc dir> <out.png> [options]".into());
    };
    let dir = Path::new(dir);
    let mut island_no = 1u32;
    let (mut px, mut pz, mut py) = (-19650i32, -10575i32, None::<i32>);
    let mut angle = 0f32;
    let mut mode = ViewMode::High;
    let mut interlace = false;
    let mut duck = Some(0x38usize);
    let mut meshes = false;
    let mut phase = 0i32;
    let mut frames = None::<u32>;
    let mut full = false;
    let mut bonus = false;
    let mut bonus_anim = 0i32;
    let mut save = None::<Vec<u8>>;
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--island" => island_no = val()?.parse()?,
            "--px" => px = val()?.parse()?,
            "--pz" => pz = val()?.parse()?,
            "--py" => py = Some(val()?.parse()?),
            "--angle" => angle = val()?.parse()?,
            "--phase" => phase = val()?.parse()?,
            "--frames" => frames = Some(val()?.parse()?),
            "--mode" => {
                mode = match val()?.as_str() {
                    "0" => ViewMode::Low13h,
                    "1" => ViewMode::LowWindow,
                    _ => ViewMode::High,
                }
            }
            "--duck" => {
                let v = val()?;
                duck = if v == "none" { None } else { Some(v.parse()?) };
            }
            "--save" => {
                let d = fs::read(val()?)?;
                let i32_at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
                island_no = i32_at(0) as u32;
                angle = f32::from_le_bytes(d[4..8].try_into().unwrap());
                px = i32_at(112);
                py = Some(i32_at(116));
                pz = i32_at(120);
                save = Some(d);
            }
            "--bonus" => bonus = true,
            "--bonus-anim" => bonus_anim = val()?.parse()?,
            "--ground" => py = None,
            "--interlace" => interlace = true,
            "--meshes" => meshes = true,
            "--full" => full = true,
            other => return Err(format!("unknown option {other}").into()),
        }
    }

    let picture = read(dir, "PICTURE.BNK")?;
    let picture = Bnk::parse(&picture).map_err(|e| e.to_string())?;
    let banque = read(dir, "BANQUE.BNK")?;
    let banque = Bnk::parse(&banque).map_err(|e| e.to_string())?;
    let island = IslandAssets::from_bytes(
        &read(dir, &format!("ILE_{island_no}.UNI"))?,
        &read(dir, &format!("ILE_{island_no}.COL"))?,
        Some((
            &read(dir, &format!("OBJETS_{island_no}.3D"))?,
            &read(dir, &format!("OBJETS_{island_no}.MAP"))?,
        )),
        picture.get(&format!("CIEL_{island_no}.RON")),
    )?;
    let duck_bank = SpriteBank::parse(banque.get("DUCK.SPT").ok_or("DUCK.SPT missing")?)?;

    let py = py.unwrap_or_else(|| terrain_height(&island, -px, -pz));
    let mut view = ViewState::default();
    match frames {
        Some(n) => (0..n).for_each(|_| view.begin_frame(py)),
        None => view.settle(py),
    }
    view.wave_phase = phase;

    // What `player_draw` queues: the horizon marker 3000 units ahead, then the duck.
    let (c, s) = ((angle as f64).cos(), (angle as f64).sin());
    let mut sprites = vec![WorldSprite {
        bank: &duck_bank,
        index: 0x2d,
        flags: 0,
        x: (c * 3000.0 + (-px) as f64) as i32,
        y: 0,
        z: (s * 3000.0 + (-pz) as f64) as i32,
        angle: 0.0,
        scale: -999.0,
        anchor: Anchor::HotSpot,
    }];
    if let Some(frame) = duck {
        sprites.push(WorldSprite {
            bank: &duck_bank,
            index: frame,
            flags: 0,
            x: -px,
            y: py,
            z: -pz,
            angle: 0.0,
            scale: -1.0,
            anchor: Anchor::HotSpot,
        });
    }
    let bonus_bank = SpriteBank::parse(
        banque.get(&format!("BONUS_{island_no}.SPT")).ok_or("BONUS_n.SPT missing")?,
    )?;
    if let (true, Some(d)) = (bonus, save.as_ref()) {
        let i32_at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let last = i32_at(428 + 20 * 4).clamp(0, 1599) as usize;
        for i in 0..=last {
            let o = 1316 + i * 20;
            let (kind, x, y, z) = (i32_at(o), i32_at(o + 8), i32_at(o + 12), i32_at(o + 16));
            if kind < 0 || ((-x) - px).abs() >= 4500 {
                continue;
            }
            let frame = match kind {
                0 => bonus_anim % 10,
                1 => 10 + bonus_anim % 4,
                2 => 14 + bonus_anim % 10,
                3 => 24 + bonus_anim % 10,
                t => t + 30,
            };
            sprites.push(WorldSprite {
                bank: &bonus_bank,
                index: frame as usize,
                flags: 0,
                x,
                y,
                z,
                angle: 0.0,
                scale: 4.0,
                anchor: Anchor::HotSpot,
            });
        }
    }
    let mut objects = Vec::new();
    if meshes && island_no == 1 {
        // Island 1 frame function `0x187e0`.
        let empty =
            ObjectInstance { active: false, model: 0, x: 0.0, y: 0.0, z: 0.0, rot_a: 0.0, rot_b: 0.0 };
        objects = vec![empty; 8];
        for (slot, model, x, z) in [
            (0, 4, 15881.0, 3225.0),
            (1, 5, 18318.0, 2700.0),
            (2, 4, 18318.0, 4481.0),
            (3, 5, 20662.0, 2981.0),
            (5, 6, -11868.0, -25050.0),
            (7, 7, -31600.0, 26850.0),
        ] {
            objects[slot] = ObjectInstance { active: true, model, x, z, ..empty };
        }
    }

    let mut renderer = Renderer::new();
    let mut info = None;
    // Twice: the sky uses the horizon row found by the previous frame.
    for _ in 0..2 {
        let scene = Scene {
            island: &island,
            mode,
            interlace,
            camera: Camera { x: px, z: pz, heading: angle },
            view,
            objects: &objects,
            sprites: &sprites,
        };
        let i = renderer.render(&scene);
        view.horizon = i.horizon;
        info = Some(i);
    }
    let mut fb = Framebuffer::new();
    fb.palette = island.palette_rgb8();
    fb.pixels.fill(pd_game::render::BORDER_COLOUR);
    let enh = if full { Enhancements::enhanced() } else { Enhancements::original() };
    renderer.present(&mut fb, mode, interlace, &enh);

    let file = BufWriter::new(fs::File::create(out)?);
    let mut enc = png::Encoder::new(file, 640, 480);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_palette(fb.palette.as_flattened().to_vec());
    let mut writer = enc.write_header()?;
    writer.write_image_data(&fb.pixels[..])?;
    writer.finish()?;
    println!(
        "{out}: island {island_no} px {px} py {py} pz {pz} angle {angle} eye_h {} look_h {} {:?}",
        view.eye_h,
        view.look_h,
        info.unwrap()
    );
    Ok(())
}
