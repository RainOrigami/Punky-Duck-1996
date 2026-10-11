//! Headless run of the game on a simulated 70 Hz clock with scripted input:
//! PNG shots, a per-tick picture hash log and the mixed audio as WAV.

use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use pd_engine::audio::CdPlayer;
use pd_engine::input::{Button, InputState, Source, scancode_from_key_code};
use pd_engine::{App, CHANNELS, Command, Framebuffer, SAMPLE_RATE};
use pd_formats::disc::Disc;
use pd_game::app::Game;

pub struct Options {
    pub ticks: u64,
    pub script: String,
    pub dump_dir: PathBuf,
    pub dump_every: u32,
    pub frame_log: Option<PathBuf>,
    pub wav: Option<PathBuf>,
}

enum Action {
    Key(String, bool),
    Shot(String),
    /// Debug hook: make the game do something no short key script reaches.
    Do(String),
}

/// Ticks a tapped key stays down.
const TAP: u64 = 4;

fn parse(script: &str) -> Result<Vec<(u64, Action)>, String> {
    let mut out = Vec::new();
    let mut t = 0u64;
    for tok in script.split_whitespace() {
        if let Some(n) = tok.strip_prefix('@') {
            t = n.parse().map_err(|_| format!("script: bad time {tok}"))?;
        } else if let Some(what) = tok.strip_prefix("do:") {
            out.push((t, Action::Do(what.to_string())));
        } else if let Some(name) = tok.strip_prefix("shot:") {
            out.push((t, Action::Shot(name.to_string())));
        } else if let Some(code) = tok.strip_prefix('+') {
            out.push((t, Action::Key(code.to_string(), true)));
        } else if let Some(code) = tok.strip_prefix('-') {
            out.push((t, Action::Key(code.to_string(), false)));
        } else {
            out.push((t, Action::Key(tok.to_string(), true)));
            out.push((t + TAP, Action::Key(tok.to_string(), false)));
        }
    }
    for (_, a) in &out {
        if let Action::Key(code, _) = a {
            if scancode_from_key_code(code).is_none() && Button::from_key_code(code).is_none() {
                return Err(format!("script: unknown key {code}"));
            }
        }
    }
    out.sort_by_key(|(t, _)| *t);
    Ok(out)
}

/// CRC-32 (zlib) of the picture as RGB24, so a log line can be compared
/// with `zlib.crc32` of a decoded reference frame.
fn crc(fb: &Framebuffer) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut h = !0u32;
    for &p in fb.pixels.iter() {
        for c in fb.palette[p as usize] {
            h = table[((h ^ c as u32) & 0xff) as usize] ^ (h >> 8);
        }
    }
    !h
}

pub fn run(mut game: Game, disc: Option<Rc<Disc>>, opts: Options) -> Result<(), String> {
    let actions = parse(&opts.script)?;
    std::fs::create_dir_all(&opts.dump_dir).map_err(|e| format!("{}: {e}", opts.dump_dir.display()))?;
    let mut log = match &opts.frame_log {
        Some(p) => Some(std::io::BufWriter::new(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?)),
        None => None,
    };
    let mut cd = CdPlayer::new(disc);
    let mut input = InputState::new();
    let mut fb = Framebuffer::new();
    let mut commands = Vec::new();
    let mut pcm: Vec<i16> = Vec::new();
    let mut audio_acc = 0u64;
    let mut next = 0;
    let mut stage = "";
    let mut traced = 0;
    // One tick of the original's timer: 17045 / 1193182 s.
    let (num, den) = (17_045u64 * 1_000_000, 1_193_182u64);
    let mut time_acc = 0u64;
    for t in 0..opts.ticks {
        let mut shots = Vec::new();
        while next < actions.len() && actions[next].0 <= t {
            match &actions[next].1 {
                Action::Key(code, down) => {
                    if let Some(b) = Button::from_key_code(code) {
                        input.set(Source::Keyboard, b, *down);
                    }
                    if let Some(sc) = scancode_from_key_code(code) {
                        game.key_event(sc, *down);
                    }
                }
                Action::Shot(name) => shots.push(name.clone()),
                Action::Do(what) => {
                    use pd_game::sim::Event;
                    let w = game.world_mut();
                    match what.as_str() {
                        "say" => w.queue_event(Event::Say(0)),
                        "picture" => w.queue_event(Event::Picture("RADEAU.RON")),
                        "timed" => w.queue_event(Event::PictureTimed("EXPLOSE.RON", 16)),
                        "shop" => w.queue_event(Event::Shop),
                        "gameover" => w.sim.s.p.lives = -1,
                        "continues" => w.sim.s.p.continues = 1,
                        "money" => w.sim.s.p.money = 123_456,
                        "ending" => w.sim.s.next_island = 2,
                        other => return Err(format!("script: unknown do:{other}")),
                    }
                }
            }
            next += 1;
        }
        time_acc += num;
        let dt = time_acc / den;
        time_acc -= dt * den;
        game.cd_status(cd.is_playing());
        game.update(dt, &input.input(), &mut commands);
        for c in commands.drain(..) {
            match c {
                Command::PlayCdTrack { track, looping } => {
                    cd.play(track, looping);
                    println!("[{t:6}] cd play {track}");
                }
                Command::StopCd => cd.stop(),
                Command::SetCdVolume { volume } => cd.set_volume((volume as u16 * 256).div_ceil(255)),
                Command::Quit => {
                    println!("[{t:6}] quit");
                }
            }
        }
        if game.stage_name() != stage {
            stage = game.stage_name();
            println!("[{t:6}] stage {stage}");
        }
        for line in &game.trace()[traced..] {
            println!("[{t:6}] {line}");
        }
        traced = game.trace().len();
        game.render(&mut fb);
        if let Some(log) = log.as_mut() {
            writeln!(log, "{t} {:08x}", crc(&fb)).map_err(|e| e.to_string())?;
        }
        for name in shots {
            let path = opts.dump_dir.join(format!("{name}.png"));
            crate::write_png(&path, &fb)?;
            println!("[{t:6}] shot {}", path.display());
        }
        if opts.dump_every > 0 && t % opts.dump_every as u64 == 0 {
            crate::write_png(&opts.dump_dir.join(format!("t{t:06}.png")), &fb)?;
        }
        if opts.wav.is_some() {
            audio_acc += SAMPLE_RATE as u64 * num;
            let frames = (audio_acc / (den * 1_000_000)) as usize;
            audio_acc -= frames as u64 * den * 1_000_000;
            let mut chunk = vec![0i16; frames * CHANNELS];
            game.mix_audio(&mut chunk);
            cd.mix_into(&mut chunk);
            pcm.extend_from_slice(&chunk);
        }
    }
    if let Some(p) = &opts.wav {
        crate::write_wav(p, &pcm)?;
        println!("audio {}", p.display());
    }
    for e in game.errors() {
        println!("error: {e}");
    }
    let sfx: Vec<String> = game.sfx_log().iter().map(|(id, v)| format!("{id}/{v}")).collect();
    println!("sfx {}", sfx.join(" "));
    println!("final stage {}", game.stage_name());
    Ok(())
}
