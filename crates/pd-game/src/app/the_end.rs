//! `THE_END.EXE`: the credits that `DUCK.BAT` runs after `P.EXE` exits.
//! Addresses are `THE_END.elf`'s.
//!
//! `main` (`0x10170`): CD track 8, `MENUSPIC.RON` as background, then for
//! every `*START*` ... `*STOP*` page of `INTRO.INI` (`0x10340`): show the
//! bare picture, fade in (64 steps), run the page (`0x10540`), fade out (128
//! steps). A key ends the page and the program.
//!
//! A page (`0x10540`): the lines sit 40 pixels apart, centred vertically.
//! Each non-empty line first flies in: 100 steps, one per retrace, in which
//! both its distance below the final row and its *letter spacing* go from
//! 200 down to 2 (the step value is passed to `font_set` as the spacing).
//! Then a band sweeps over its row in 40 steps: a darkened band from the
//! left for the first line (drawn with `COOLFNT2`), a brightened band from
//! the right for the others (`COOLFNT1`). After the last line: 8 seconds.

use super::core::Core;
use super::gfx::{self, Align, BLACK, Fade, FontState, Pal, blend_table};

/// Lines per page the reader accepts before it stops by itself (`0x10520`).
const MAX_LINES: usize = 16;

/// `0x10410`: the pages of `INTRO.INI`. Lines are read with `fgets(.., 32)`
/// and lose their last two characters (CR LF).
pub fn parse_pages(ini: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut pages = Vec::new();
    let mut pos = 0;
    let fgets = |pos: &mut usize, size: usize| -> Option<Vec<u8>> {
        if *pos >= ini.len() {
            return None;
        }
        let rest = &ini[*pos..];
        let limit = rest.len().min(size - 1);
        let n = rest[..limit].iter().position(|&b| b == b'\n').map_or(limit, |p| p + 1);
        *pos += n;
        Some(rest[..n].to_vec())
    };
    'pages: loop {
        loop {
            match fgets(&mut pos, 0x80) {
                None => break 'pages,
                Some(l) if l.starts_with(b"*START*") => break,
                Some(_) => {}
            }
        }
        let mut lines: Vec<Vec<u8>> = Vec::new();
        loop {
            let Some(mut l) = fgets(&mut pos, 0x20) else { break 'pages };
            l.pop();
            l.pop();
            if l.starts_with(b"*STOP*") || lines.len() >= MAX_LINES {
                break;
            }
            lines.push(l);
        }
        pages.push(lines);
    }
    pages
}

enum State {
    Start,
    /// Fade in, 65 retraces.
    FadeIn(Fade),
    /// Line `line`, fly-in offset `off` (200, 198, .. 2).
    Fly { line: usize, off: i32 },
    /// Line `line`, band step `s` (0, 8, .. 312), then the final full-width draw.
    Sweep { line: usize, s: i32 },
    /// `delay(8000)` in ticks.
    Wait(u32),
    FadeOut(Fade),
    Done,
}

pub struct TheEnd {
    pages: Vec<Vec<Vec<u8>>>,
    page: usize,
    state: State,
    pal: Pal,
    bg: Vec<u8>,
    dark: [u8; 256],
    light: [u8; 256],
    /// A key was pressed (`0x10c660`): finish the current fade-out and stop.
    aborted: bool,
}

impl TheEnd {
    pub fn new(core: &mut Core) -> Self {
        let pages = core.assets.load("INTRO.INI").map(|d| parse_pages(&d)).unwrap_or_default();
        Self {
            pages,
            page: 0,
            state: State::Start,
            pal: BLACK,
            bg: Vec::new(),
            dark: [0; 256],
            light: [0; 256],
            aborted: false,
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn start(&mut self, core: &mut Core) {
        core.note("the_end");
        core.kbd.set_bios(true);
        core.kbd.flush();
        core.cd_stop();
        core.cd_play(8);
        core.video.set_palette(&BLACK);
        core.video.fill_screen(0);
        if let Some(p) = core.load_picture("MENUSPIC.RON") {
            self.pal = p.pal;
            self.bg = vec![0; gfx::W * gfx::H];
            for (y, row) in p.pixels.chunks_exact(p.w.max(1)).take(gfx::H).enumerate() {
                let n = p.w.min(gfx::W);
                self.bg[y * gfx::W..y * gfx::W + n].copy_from_slice(&row[..n]);
            }
        } else {
            self.bg = vec![0; gfx::W * gfx::H];
        }
        self.dark = blend_table([0, 0, 0], 0x10, &self.pal);
        self.light = blend_table([0x3f, 0x3f, 0x3f], 0x10, &self.pal);
        core.coolfnt1 = core.load_sheet("COOLFNT1.SPT");
        core.coolfnt2 = core.load_sheet("COOLFNT2.SPT");
        core.coolfnt3 = core.load_sheet("COOLFNT3.SPT");
        // sprite_set_target(.., 0, 0, 639, 0x1d6)
        core.video.clip_y0 = 0;
        core.video.clip_y1 = 0x1d6;
        core.video.back.copy_from_slice(&self.bg);
        self.begin_page(core);
    }

    fn begin_page(&mut self, core: &mut Core) {
        if self.page >= self.pages.len() || self.aborted {
            return self.finish(core);
        }
        core.video.back.copy_from_slice(&self.bg);
        core.video.present();
        self.state = State::FadeIn(Fade::new(0, 0x40, &BLACK, &self.pal));
    }

    fn finish(&mut self, core: &mut Core) {
        core.cd_stop();
        core.kbd.flush();
        core.video.fill_screen(0);
        core.video.set_palette(&BLACK);
        core.video.clip_y1 = gfx::H as i32 - 1;
        self.state = State::Done;
    }

    /// First line at or after `from` that is not empty, with its row.
    fn next_line(&self, from: usize) -> Option<usize> {
        let lines = &self.pages[self.page];
        (from..lines.len()).find(|&k| !lines[k].is_empty())
    }

    fn row(&self, line: usize, off: i32) -> (i32, i32) {
        let n = self.pages[self.page].len() as i32;
        let y = (n * -0x28 + 0x1e0) / 2 + line as i32 * 0x28 + off;
        (y, (y + 0x28).min(0x1df))
    }

    fn restore_rows(&self, core: &mut Core, y: i32, y2: i32) {
        for yy in y.max(0)..=y2 {
            let o = yy as usize * gfx::W;
            core.video.back[o..o + gfx::W].copy_from_slice(&self.bg[o..o + gfx::W]);
        }
    }

    fn draw_text(&self, core: &mut Core, line: usize, y: i32, spacing: i32) {
        let sheet = if line == 0 { &core.coolfnt2 } else { &core.coolfnt1 };
        let font = FontState::set(sheet, 0x20, spacing);
        core.video.text(sheet, &font, 0x145, y + 0x1e, &self.pages[self.page][line], Align::Centre);
    }

    fn key(&mut self, core: &mut Core) -> bool {
        if core.kbd.getkey().is_some() {
            self.aborted = true;
        }
        self.aborted
    }

    /// One retrace.
    pub fn tick(&mut self, core: &mut Core) {
        match &mut self.state {
            State::Done => {}
            State::Start => self.start(core),
            State::FadeIn(f) => {
                if f.step(&mut core.video.dac) {
                    self.state = match self.next_line(0) {
                        Some(line) => State::Fly { line, off: 200 },
                        None => State::Wait(560),
                    };
                }
            }
            State::Fly { line, off } => {
                let (line, off) = (*line, *off);
                let (y, y2) = self.row(line, off);
                if y < 0x1e0 {
                    self.restore_rows(core, y, y2);
                    self.draw_text(core, line, y, off);
                    core.video.present_rect(0, y, 0x27f, y2);
                }
                if self.key(core) {
                    return self.leave_page(core);
                }
                self.state = if off - 2 > 0 { State::Fly { line, off: off - 2 } } else { State::Sweep { line, s: 0 } };
            }
            State::Sweep { line, s } => {
                let (line, s) = (*line, *s);
                // The row of the last fly-in step (offset 2).
                let (y, y2) = self.row(line, 2);
                self.restore_rows(core, y, y2);
                let last = s >= 0x140;
                let (x0, x1) = match (line == 0, last) {
                    (_, true) => (0, 0x27f),
                    (true, false) => (0, s * 2),
                    (false, false) => (0x27f - s * 2, 0x27f),
                };
                let table = if line == 0 { self.dark } else { self.light };
                core.video.remap_rect(&table, x0, y, x1, y2 - 1);
                self.draw_text(core, line, y, 2);
                core.video.present_rect(0, y, 0x27f, y2);
                if last {
                    self.state = match self.next_line(line + 1) {
                        Some(line) => State::Fly { line, off: 200 },
                        None => State::Wait(560),
                    };
                    return;
                }
                if self.key(core) {
                    return self.leave_page(core);
                }
                self.state = State::Sweep { line, s: s + 8 };
            }
            State::Wait(left) => {
                // delay() does not look at the keyboard.
                if *left == 0 {
                    self.leave_page(core);
                } else {
                    *left -= 1;
                }
            }
            State::FadeOut(f) => {
                if f.step(&mut core.video.dac) {
                    self.page += 1;
                    self.begin_page(core);
                }
            }
        }
    }

    fn leave_page(&mut self, _core: &mut Core) {
        self.state = State::FadeOut(Fade::new(0, 0x80, &self.pal, &BLACK));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_split_like_the_original_reader() {
        let ini = b"# c\r\n*START*\r\nA B\r\n\r\nCD\r\n*STOP*\r\n\r\n*START*\r\nX\r\n*STOP*\r\n";
        let pages = parse_pages(ini);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0], vec![b"A B".to_vec(), b"".to_vec(), b"CD".to_vec()]);
        assert_eq!(pages[1], vec![b"X".to_vec()]);
    }
}
