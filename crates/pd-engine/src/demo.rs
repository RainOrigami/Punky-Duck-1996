//! Stand-in program until the game logic exists: plays the publisher logo and
//! the intro movie with CD music, then shows a test pattern that reflects the
//! input state and exercises the SFX mixer.
//!
//! Which CD track belongs to which movie is a **hypothesis** based on
//! durations only (C_SOFT.MOV 10.1 s / track 2 15.0 s; INTRO.MOV 63.8 s /
//! track 3 69.2 s); the real assignment is in `INTRO.EXE`.

use crate::audio::{Mixer, PAN_RANGE, Sample, VoiceParams};
use crate::movie::MoviePlayer;
use crate::{App, AssetSource, Command, Framebuffer, Input, SCREEN_H, SCREEN_W};

struct Stage {
    file: &'static str,
    track: u8,
    looping: bool,
}

const STAGES: [Stage; 2] = [
    Stage { file: "C_SOFT.MOV", track: 2, looping: false },
    Stage { file: "INTRO.MOV", track: 3, looping: true },
];

// Test pattern palette indices.
const BLACK: u8 = 0;
const WHITE: u8 = 1;
const DARK: u8 = 2;
const GREY: u8 = 3;
const LIT: u8 = 4;
const RED: u8 = 5;
const BARS: u8 = 16; // 8 colour bars
const RAMP: u8 = 32; // 64 grey levels

pub struct DemoApp {
    /// Movies still to play, in order.
    movies: Vec<(MoviePlayer, &'static Stage)>,
    /// Index into `movies` of the one playing; `movies.len()` = test pattern.
    current: usize,
    started: bool,
    errors: Vec<String>,
    mixer: Mixer,
    blip: Sample,
    input: Input,
    prev_input: Input,
    time_us: u64,
}

impl DemoApp {
    pub fn new(assets: &dyn AssetSource) -> Self {
        let mut movies = Vec::new();
        let mut errors = Vec::new();
        for stage in &STAGES {
            match assets.load(stage.file) {
                None => errors.push(format!("{}: not found", stage.file)),
                Some(data) => match MoviePlayer::new(data) {
                    Ok(player) => movies.push((player, stage)),
                    Err(e) => errors.push(format!("{}: {e}", stage.file)),
                },
            }
        }
        Self {
            movies,
            current: 0,
            started: false,
            errors,
            mixer: Mixer::new(8),
            blip: make_blip(),
            input: Input::default(),
            prev_input: Input::default(),
            time_us: 0,
        }
    }

    /// Movies that could not be loaded or decoded. Frontends report these.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Index of the stage that is running: one per loaded movie, then the
    /// test pattern (`stage_count() - 1`).
    pub fn stage(&self) -> usize {
        self.current.min(self.movies.len())
    }

    pub fn stage_count(&self) -> usize {
        self.movies.len() + 1
    }

    /// File name of the movie playing in the current stage, if any.
    pub fn stage_name(&self) -> &'static str {
        self.movies.get(self.current).map_or("test pattern", |(_, s)| s.file)
    }

    /// True once the movies are over and the test pattern is showing.
    pub fn in_test_pattern(&self) -> bool {
        self.started && self.current >= self.movies.len()
    }

    fn enter_stage(&self, commands: &mut Vec<Command>) {
        if let Some((_, stage)) = self.movies.get(self.current) {
            commands.push(Command::PlayCdTrack { track: stage.track, looping: stage.looping });
        } else if self.movies.is_empty() {
            // No movie started any music; still give the pattern a track.
            let last = &STAGES[STAGES.len() - 1];
            commands.push(Command::PlayCdTrack { track: last.track, looping: true });
        }
    }
}

/// 0.12 s decaying 880 Hz triangle at 11025 Hz mono, so the mixer's
/// resampling path is exercised.
fn make_blip() -> Sample {
    const RATE: u32 = 11_025;
    let n = (RATE as usize * 12) / 100;
    let period = RATE as i32 * 256 / 880; // in 1/256 samples
    let data: Vec<i16> = (0..n)
        .map(|i| {
            let phase = (i as i32 * 256) % period; // 0..period
            let tri = if phase * 2 < period {
                -8000 + 32000 * phase / period
            } else {
                24000 - 32000 * phase / period
            };
            (tri * (n - i) as i32 / n as i32) as i16
        })
        .collect();
    Sample::mono(data, RATE)
}

impl App for DemoApp {
    fn update(&mut self, dt_us: u64, input: &Input, commands: &mut Vec<Command>) {
        if !self.started {
            self.started = true;
            self.enter_stage(commands);
        }
        self.time_us += dt_us;
        self.prev_input = self.input;
        self.input = *input;

        if let Some((movie, stage)) = self.movies.get_mut(self.current) {
            movie.update(dt_us, input, commands);
            if movie.is_finished() {
                if let Some(e) = movie.error() {
                    self.errors.push(format!("{}: {e}", stage.file));
                }
                self.current += 1;
                self.enter_stage(commands);
            }
            return;
        }

        // Test pattern: a blip per fresh press, panned for left/right.
        let (now, before) = (&self.input, &self.prev_input);
        let mut blip = |pressed: bool, pan: i16| {
            if pressed {
                self.mixer.play(&self.blip, VoiceParams { pan, ..VoiceParams::default() });
            }
        };
        blip(now.action && !before.action, 0);
        blip(now.item && !before.item, 0);
        blip(now.mouse_left && !before.mouse_left, 0);
        blip(now.left && !before.left, -PAN_RANGE);
        blip(now.right && !before.right, PAN_RANGE);
    }

    fn render(&mut self, fb: &mut Framebuffer) {
        if let Some((movie, _)) = self.movies.get_mut(self.current) {
            movie.render(fb);
            return;
        }
        draw_pattern(fb, &self.input, self.time_us, !self.errors.is_empty());
    }

    fn mix_audio(&mut self, out: &mut [i16]) {
        self.mixer.fill(out);
    }
}

fn rect(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: u8) {
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + w).min(SCREEN_W as i32), (y + h).min(SCREEN_H as i32));
    for yy in y0..y1 {
        let row = yy as usize * SCREEN_W;
        if x0 < x1 {
            fb.pixels[row + x0 as usize..row + x1 as usize].fill(c);
        }
    }
}

/// 3x5 glyph rows, top to bottom, bit 2 = left column.
fn glyph(ch: char) -> [u8; 5] {
    match ch {
        'A' => [2, 5, 7, 5, 5],
        'B' => [6, 5, 6, 5, 6],
        'C' => [3, 4, 4, 4, 3],
        'D' => [6, 5, 5, 5, 6],
        'E' => [7, 4, 6, 4, 7],
        'F' => [7, 4, 6, 4, 4],
        'G' => [3, 4, 5, 5, 3],
        'H' => [5, 5, 7, 5, 5],
        'I' => [7, 2, 2, 2, 7],
        'J' => [1, 1, 1, 5, 2],
        'K' => [5, 5, 6, 5, 5],
        'L' => [4, 4, 4, 4, 7],
        'M' => [5, 7, 7, 5, 5],
        'N' => [6, 5, 5, 5, 5],
        'O' => [2, 5, 5, 5, 2],
        'P' => [6, 5, 6, 4, 4],
        'Q' => [2, 5, 5, 6, 3],
        'R' => [6, 5, 6, 5, 5],
        'S' => [3, 4, 2, 1, 6],
        'T' => [7, 2, 2, 2, 2],
        'U' => [5, 5, 5, 5, 7],
        'V' => [5, 5, 5, 5, 2],
        'W' => [5, 5, 7, 7, 5],
        'X' => [5, 5, 2, 5, 5],
        'Y' => [5, 5, 2, 2, 2],
        'Z' => [7, 1, 2, 4, 7],
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 1, 1],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        '-' => [0, 0, 7, 0, 0],
        '.' => [0, 0, 0, 0, 2],
        ':' => [0, 2, 0, 2, 0],
        _ => [0; 5],
    }
}

fn text(fb: &mut Framebuffer, x: i32, y: i32, scale: i32, c: u8, s: &str) {
    for (i, ch) in s.chars().enumerate() {
        let gx = x + i as i32 * 4 * scale;
        for (row, bits) in glyph(ch.to_ascii_uppercase()).iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    rect(fb, gx + col * scale, y + row as i32 * scale, scale, scale, c);
                }
            }
        }
    }
}

fn button(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, on: bool, label: &str) {
    rect(fb, x, y, w, h, WHITE);
    rect(fb, x + 2, y + 2, w - 4, h - 4, if on { LIT } else { DARK });
    let tw = label.len() as i32 * 8 - 2;
    text(fb, x + (w - tw) / 2, y + (h - 10) / 2, 2, if on { BLACK } else { WHITE }, label);
}

fn draw_pattern(fb: &mut Framebuffer, input: &Input, time_us: u64, had_errors: bool) {
    let pal = &mut fb.palette;
    *pal = [[0; 3]; 256];
    pal[WHITE as usize] = [255, 255, 255];
    pal[DARK as usize] = [40, 40, 48];
    pal[GREY as usize] = [128, 128, 128];
    pal[LIT as usize] = [64, 255, 96];
    pal[RED as usize] = [255, 48, 48];
    let bars: [[u8; 3]; 8] = [
        [255, 255, 255],
        [255, 255, 0],
        [0, 255, 255],
        [0, 255, 0],
        [255, 0, 255],
        [255, 0, 0],
        [0, 0, 255],
        [0, 0, 0],
    ];
    for (i, c) in bars.iter().enumerate() {
        pal[BARS as usize + i] = *c;
    }
    for i in 0..64u8 {
        let v = (i << 2) | (i >> 4);
        pal[(RAMP + i) as usize] = [v, v, v];
    }

    fb.pixels.fill(BLACK);
    // Colour bars, grey ramp.
    for i in 0..8 {
        rect(fb, i * 80, 0, 80, 100, BARS + i as u8);
    }
    for i in 0..64 {
        rect(fb, i * 10, 100, 10, 40, RAMP + i as u8);
    }
    // 1-pixel checkerboard and line pairs: reveal non-nearest or uneven scaling.
    for y in 150..182 {
        for x in 8..72 {
            fb.pixels[y * SCREEN_W + x] = if (x + y) % 2 == 0 { WHITE } else { BLACK };
        }
        for x in 88..152 {
            fb.pixels[y * SCREEN_W + x] = if x % 2 == 0 { WHITE } else { BLACK };
        }
        for x in 168..232 {
            fb.pixels[y * SCREEN_W + x] = if y % 2 == 0 { WHITE } else { BLACK };
        }
    }
    // Block moving at 160 px/s: judder shows uneven frame pacing.
    let x = (time_us * 160 / 1_000_000 % (SCREEN_W as u64 + 40)) as i32 - 40;
    rect(fb, x, 192, 40, 16, WHITE);
    // Seconds counter (ticks at real-time rate if the clock is right).
    let secs = time_us / 1_000_000;
    text(fb, 250, 152, 4, WHITE, &format!("T {:02}:{:02}.{}", secs / 60, secs % 60, time_us / 100_000 % 10));

    text(fb, 8, 222, 3, WHITE, "PUNKY DUCK ENGINE TEST");
    if had_errors {
        text(fb, 330, 222, 3, RED, "MOVIE ERROR");
    }

    // D-pad cluster.
    button(fb, 108, 250, 90, 36, input.up, "UP");
    button(fb, 8, 290, 90, 36, input.left, "LEFT");
    button(fb, 208, 290, 90, 36, input.right, "RIGHT");
    button(fb, 108, 330, 90, 36, input.down, "DOWN");
    // Buttons, labelled with the Input field and its default key.
    let cells: [(bool, &str); 14] = [
        (input.action, "ACTION"),
        (input.secondary, "SECONDARY"),
        (input.inventory, "INVENTORY"),
        (input.cancel, "CANCEL"),
        (input.item, "ITEM"),
        (input.next_item, "NEXT ITEM"),
        (input.view, "VIEW"),
        (input.interlace, "INTERLACE"),
        (input.misc, "MISC"),
        (input.key1, "KEY 1"),
        (input.key2, "KEY 2"),
        (input.pause, "PAUSE"),
        (input.mouse_left, "MOUSE L"),
        (input.mouse_right, "MOUSE R"),
    ];
    for (i, (on, label)) in cells.into_iter().enumerate() {
        let (col, row) = ((i % 3) as i32, (i / 3) as i32);
        button(fb, 320 + col * 106, 250 + row * 34, 100, 30, on, label);
    }

    text(fb, 8, 430, 2, GREY, &format!("MOUSE {:3} {:3}", input.mouse_x, input.mouse_y));
    text(fb, 8, 450, 2, GREY, "ACTION ITEM LEFT RIGHT MOUSE L: SFX BLIP");

    // Frame corners: the outermost pixels must be visible after scaling.
    for (cx, cy) in [(0, 0), (SCREEN_W as i32 - 8, 0), (0, SCREEN_H as i32 - 8), (SCREEN_W as i32 - 8, SCREEN_H as i32 - 8)] {
        rect(fb, cx, cy, 8, 8, RED);
    }
    // Mouse crosshair.
    let (mx, my) = (input.mouse_x, input.mouse_y);
    rect(fb, mx - 8, my, 17, 1, RED);
    rect(fb, mx, my - 8, 1, 17, RED);
}
