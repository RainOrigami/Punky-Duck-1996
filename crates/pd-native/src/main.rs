//! Native Linux frontend: window, GPU presentation, audio device, input
//! devices, files. Everything else lives in the portable crates.

mod assets;
mod audio;
mod gamepad;
mod headless;
mod present;
mod window;

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use pd_engine::demo::DemoApp;
use pd_engine::runner::Runner;
use pd_engine::{App, AssetSource, CHANNELS, Command, Framebuffer, Input, SAMPLE_RATE, SCREEN_H, SCREEN_W};
use pd_engine::assets::DiscAssets;
use pd_formats::disc::Disc;
use pd_formats::flc::Flc;
use pd_game::app::{Game, GameOptions};

const USAGE: &str = "\
pd-native: Punky Duck (native frontend)

USAGE:
    pd-native --disc <PUNKYDUCK.cue|PUNKYDUCK.bin> [options]
    pd-native --dir <extracted PUNKY directory> [options]     (no CD music)

OPTIONS:
    --fullscreen          start in fullscreen (toggle: F11 or Alt+Enter)
    --integer             scale by whole factors only (toggle: F10); default is
                          the largest 4:3 picture that fits. Always nearest-
                          neighbour, letterboxed.
    --no-audio            do not open an audio device
    --trace               print stage changes and loop statistics
    --exit-after <secs>   quit after this many seconds
    --original            original behaviour, bugs included (default; remembered)
    --enhanced            fixes and improvements on (remembered)
    --no-intro            skip INTRO.EXE's movies
    --no-credits          skip THE_END.EXE's credits after EXIT
    --frame-ticks <n>     70 Hz ticks per rendered island frame (default 4;
                          8 = the DOSBox-X reference machine)
    --storage <dir>       where GAME.STP and the saves live
                          (default $XDG_DATA_HOME/punkyduck)
    --demo                run the engine demo instead of the game

HEADLESS (no window, simulated 70 Hz clock):
    --headless            run the game without window or audio device
    --ticks <n>           length of the run in 70 Hz ticks (default 700)
    --script <text>       input script: @<tick> sets the time, <Code> taps a
                          key (KeyboardEvent.code names: Enter, ArrowDown, KeyT,
                          ShiftLeft ...), +<Code> / -<Code> hold / release,
                          shot:<name> writes <dump-dir>/<name>.png, do:<say|picture|
                          timed|shop|gameover|continues|money|ending> forces
                          that game event (debugging)
    --dump-dir <dir>      where shots go (default .)
    --dump-every <n>      also write every n-th tick as tNNNNNN.png
    --frame-log <file>    one line per tick: CRC-32 of the shown picture as RGB24
    --dump-wav <file>     write the mixed audio of the run

DIAGNOSTICS (no window):
    --dump-frames <dir>   write first/middle/last frame of every .MOV as PNG
    --dump-every <n>      with --dump-frames: also write every n-th frame
    --dump-wav <file>     run the demo headless and write the mixed audio
    --seconds <n>         length of the --dump-wav run (default 30)

KEYS (the original's; see docs/re/engine/input.md):
    arrows / WASD  move          Ctrl       action (fire)
    Space          secondary     Enter      item / confirm
    Tab            inventory     Backspace  next item
    Esc            cancel        F1 / F2    view / interlace
    P              pause         1, 2, Home..Del
    Ctrl, Space, Enter, Esc or a click skip movies.
";

#[derive(Default)]
struct Cli {
    disc: Option<PathBuf>,
    dir: Option<PathBuf>,
    fullscreen: bool,
    integer: bool,
    no_audio: bool,
    trace: bool,
    exit_after: Option<f64>,
    dump_frames: Option<PathBuf>,
    dump_wav: Option<PathBuf>,
    seconds: f64,
    dump_every: u32,
    mode: Option<pd_game::app::Mode>,
    no_intro: bool,
    no_credits: bool,
    frame_ticks: Option<u32>,
    storage: Option<PathBuf>,
    demo: bool,
    headless: bool,
    ticks: u64,
    script: String,
    dump_dir: Option<PathBuf>,
    frame_log: Option<PathBuf>,
}

fn parse_cli() -> Result<Cli, String> {
    let mut cli = Cli { seconds: 30.0, ticks: 700, ..Cli::default() };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--disc" => cli.disc = Some(value("a path")?.into()),
            "--dir" => cli.dir = Some(value("a path")?.into()),
            "--fullscreen" => cli.fullscreen = true,
            "--integer" => cli.integer = true,
            "--no-audio" => cli.no_audio = true,
            "--trace" => cli.trace = true,
            "--exit-after" => {
                cli.exit_after = Some(value("seconds")?.parse().map_err(|_| "--exit-after: not a number")?)
            }
            "--dump-frames" => cli.dump_frames = Some(value("a directory")?.into()),
            "--dump-wav" => cli.dump_wav = Some(value("a file")?.into()),
            "--dump-every" => cli.dump_every = value("a count")?.parse().map_err(|_| "--dump-every: not a number")?,
            "--seconds" => cli.seconds = value("seconds")?.parse().map_err(|_| "--seconds: not a number")?,
            "--original" => cli.mode = Some(pd_game::app::Mode::Original),
            "--enhanced" => cli.mode = Some(pd_game::app::Mode::Enhanced),
            "--no-intro" => cli.no_intro = true,
            "--no-credits" => cli.no_credits = true,
            "--frame-ticks" => {
                cli.frame_ticks = Some(value("a count")?.parse().map_err(|_| "--frame-ticks: not a number")?)
            }
            "--storage" => cli.storage = Some(value("a directory")?.into()),
            "--demo" => cli.demo = true,
            "--headless" => cli.headless = true,
            "--ticks" => cli.ticks = value("a count")?.parse().map_err(|_| "--ticks: not a number")?,
            "--script" => cli.script = value("a script")?,
            "--dump-dir" => cli.dump_dir = Some(value("a directory")?.into()),
            "--frame-log" => cli.frame_log = Some(value("a file")?.into()),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    match (&cli.disc, &cli.dir) {
        (Some(_), Some(_)) => Err("give either --disc or --dir, not both".into()),
        (None, None) => Err("one of --disc or --dir is required".into()),
        _ => Ok(cli),
    }
}

/// Delegates to the demo and publishes its stage for `--trace`.
struct Probed {
    inner: DemoApp,
    probe: window::StageProbe,
}

impl App for Probed {
    fn update(&mut self, dt_us: u64, input: &Input, commands: &mut Vec<Command>) {
        self.inner.update(dt_us, input, commands);
        self.probe.set((self.inner.stage(), self.inner.stage_name()));
    }
    fn render(&mut self, fb: &mut Framebuffer) {
        self.inner.render(fb);
    }
    fn mix_audio(&mut self, out: &mut [i16]) {
        self.inner.mix_audio(out);
    }
}

/// Delegates to the game and publishes its stage for `--trace`.
struct ProbedGame {
    inner: Game,
    probe: window::StageProbe,
    last: &'static str,
    count: usize,
}

impl App for ProbedGame {
    fn update(&mut self, dt_us: u64, input: &Input, commands: &mut Vec<Command>) {
        self.inner.update(dt_us, input, commands);
        let name = self.inner.stage_name();
        if name != self.last {
            self.last = name;
            self.count += 1;
        }
        self.probe.set((self.count, name));
    }
    fn render(&mut self, fb: &mut Framebuffer) {
        self.inner.render(fb);
    }
    fn mix_audio(&mut self, out: &mut [i16]) {
        self.inner.mix_audio(out);
    }
    fn key_event(&mut self, scancode: u8, pressed: bool) {
        self.inner.key_event(scancode, pressed);
    }
    fn cd_status(&mut self, playing: bool) {
        self.inner.cd_status(playing);
    }
}

pub(crate) fn write_png(path: &Path, fb: &Framebuffer) -> Result<(), String> {
    let mut rgba = vec![0u8; SCREEN_W * SCREEN_H * 4];
    fb.to_rgba(&mut rgba);
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SCREEN_W as u32, SCREEN_H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}

/// Decode every `.MOV` completely; save first, middle and last frame.
fn dump_frames(assets: &dyn AssetSource, dir: &Path, every: u32) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for name in ["C_SOFT.MOV", "INTRO.MOV", "PART_1.MOV", "PART_2.MOV", "PUNKY.MOV"] {
        let data = assets.load(name).ok_or(format!("{name}: not found"))?;
        let mut flc = Flc::new(data).map_err(|e| format!("{name}: {e}"))?;
        let h = *flc.header();
        if (h.width as usize, h.height as usize) != (SCREEN_W, SCREEN_H) {
            return Err(format!("{name}: unexpected size {}x{}", h.width, h.height));
        }
        let total = h.frames as u32;
        let wanted = [0, total / 2, total - 1];
        let mut fb = Framebuffer::new();
        while let Some(info) = flc.decode_next().map_err(|e| format!("{name}: {e}"))? {
            if wanted.contains(&info.index) || (every > 0 && info.index % every == 0) {
                fb.pixels.copy_from_slice(flc.pixels());
                fb.palette = *flc.palette();
                let stem = name.trim_end_matches(".MOV").to_lowercase();
                let out = dir.join(format!("{stem}_{:04}.png", info.index));
                write_png(&out, &fb)?;
                println!("{}", out.display());
            }
        }
        println!("{name}: {total} frames at {} ms", h.speed_ms);
    }
    Ok(())
}

/// Run the demo with a simulated 60 Hz clock and no input; write what the
/// audio device would have received, plus the final picture next to it.
fn dump_wav(mut runner: Runner, path: &Path, seconds: f64) -> Result<(), String> {
    const TICK_US: u64 = 1_000_000 / 60;
    let frames_per_tick = SAMPLE_RATE as usize / 60;
    let ticks = (seconds * 60.0) as usize;
    let mut pcm: Vec<i16> = Vec::with_capacity(ticks * frames_per_tick * CHANNELS);
    let mut chunk = vec![0i16; frames_per_tick * CHANNELS];
    for _ in 0..ticks {
        runner.tick(TICK_US, &Input::default());
        runner.mix_audio(&mut chunk);
        pcm.extend_from_slice(&chunk);
    }
    write_wav(path, &pcm)?;
    let png = path.with_extension("png");
    write_png(&png, runner.render())?;
    println!("{} ({seconds} s), final picture {}", path.display(), png.display());
    Ok(())
}

pub(crate) fn write_wav(path: &Path, pcm: &[i16]) -> Result<(), String> {
    let data_len = (pcm.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&(CHANNELS as u16).to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * CHANNELS as u32 * 2).to_le_bytes());
    wav.extend_from_slice(&((CHANNELS * 2) as u16).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    pcm.iter().for_each(|s| wav.extend_from_slice(&s.to_le_bytes()));
    std::fs::write(path, wav).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let cli = parse_cli()?;

    // Assets and, with a disc image, the CD audio.
    let (assets, disc): (Box<dyn AssetSource>, Option<Rc<Disc>>) = if let Some(path) = &cli.disc {
        let disc = assets::load_disc(path)?;
        let assets = DiscAssets::new(disc.clone());
        eprintln!(
            "pd-native: disc \"{}\", {} files in /{}, audio tracks {:?}",
            disc.volume_id(),
            assets.names().count(),
            assets.base(),
            disc.audio_track_numbers()
        );
        (Box::new(assets), Some(disc))
    } else {
        let dir = cli.dir.as_ref().unwrap();
        let assets = assets::load_dir(dir)?;
        eprintln!("pd-native: {} files from {}; no disc image, so no CD music", assets.len(), dir.display());
        (Box::new(assets), None)
    };

    if let Some(dir) = &cli.dump_frames {
        return dump_frames(assets.as_ref(), dir, cli.dump_every);
    }

    let probe: window::StageProbe = Rc::new(Cell::new((0, "")));
    let runner = if cli.demo {
        let demo = DemoApp::new(assets.as_ref());
        for e in demo.errors() {
            eprintln!("pd-native: {e}");
        }
        Runner::new(Box::new(Probed { inner: demo, probe: probe.clone() }), disc)
    } else {
        // Settings and saves; the original/enhanced choice is remembered in
        // PORT.CFG next to them (GAME.STP keeps the original's format).
        let mut storage: Box<dyn pd_engine::Storage> = match (&cli.storage, cli.headless) {
            (Some(dir), _) => Box::new(assets::FileStorage::at(dir.clone())),
            (None, true) => Box::new(pd_game::app::store::MemStorage::default()),
            (None, false) => Box::new(assets::FileStorage::new()),
        };
        let mode = pd_game::app::Mode::select(cli.mode, storage.as_mut());
        let mut opts = GameOptions { mode, intro: !cli.no_intro, credits: !cli.no_credits, ..GameOptions::default() };
        if let Some(n) = cli.frame_ticks {
            opts.frame_ticks = n.max(1);
        }
        eprintln!("pd-native: {} mode, {} ticks per island frame", mode.name(), opts.frame_ticks);
        let game = Game::new(assets, storage, opts);
        for e in game.errors() {
            eprintln!("pd-native: {e}");
        }
        if cli.headless {
            return headless::run(
                game,
                disc,
                headless::Options {
                    ticks: cli.ticks,
                    script: cli.script.clone(),
                    dump_dir: cli.dump_dir.clone().unwrap_or_else(|| PathBuf::from(".")),
                    dump_every: cli.dump_every,
                    frame_log: cli.frame_log.clone(),
                    wav: cli.dump_wav.clone(),
                },
            );
        }
        Runner::new(Box::new(ProbedGame { inner: game, probe: probe.clone(), last: "", count: 0 }), disc)
    };

    if let Some(path) = &cli.dump_wav {
        return dump_wav(runner, path, cli.seconds);
    }

    window::run(
        runner,
        probe,
        window::Options {
            fullscreen: cli.fullscreen,
            integer: cli.integer,
            audio: !cli.no_audio,
            trace: cli.trace,
            exit_after: cli.exit_after.map(Duration::from_secs_f64),
        },
    )
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pd-native: {e}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
