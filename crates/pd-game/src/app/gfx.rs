//! The original's 2D layer as the program-flow code uses it: back buffer,
//! VESA screen, 6-bit DAC, sprite blitter, fonts, palette fades.
//!
//! Addresses are `P.EXE` link addresses (`docs/re/functions.md`).

use pd_engine::Framebuffer;
use pd_formats::spt::SpriteSheet;

pub const W: usize = 640;
pub const H: usize = 480;

/// 256 colours with 6-bit components, as programmed into the VGA DAC.
pub type Pal = [[u8; 3]; 256];

pub const BLACK: Pal = [[0; 3]; 256];

/// A sprite sheet decoded for the blitter (`.SPT`, `docs/re/formats/spt.md`).
pub struct Sheet {
    sprites: Vec<Spr>,
}

struct Spr {
    w: i32,
    h: i32,
    hot_x: i32,
    hot_y: i32,
    /// Per row: runs of `(x, pixels)`.
    rows: Vec<Vec<(i32, Box<[u8]>)>>,
}

impl Sheet {
    /// Sprites that fail to decode become empty (the original would draw garbage).
    pub fn parse(data: &[u8]) -> Option<Sheet> {
        let sheet = SpriteSheet::parse(data).ok()?;
        let sprites = sheet
            .entries()
            .iter()
            .enumerate()
            .map(|(i, e)| Spr {
                w: e.width as i32,
                h: e.height as i32,
                hot_x: e.hot_x as i32,
                hot_y: e.hot_y as i32,
                rows: sheet
                    .runs(i)
                    .map(|rows| {
                        rows.into_iter()
                            .map(|runs| runs.into_iter().map(|r| (r.x as i32, r.pixels.into())).collect())
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        Some(Sheet { sprites })
    }

    pub fn empty() -> Sheet {
        Sheet { sprites: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Sprite width as the directory stores it (0 if out of range).
    pub fn width(&self, index: usize) -> i32 {
        self.sprites.get(index).map_or(0, |s| s.w)
    }

    pub fn height(&self, index: usize) -> i32 {
        self.sprites.get(index).map_or(0, |s| s.h)
    }
}

/// Text alignment argument of `text_draw` (`0x32ce0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left = 0,
    Centre = 1,
    Right = 2,
}

/// Font state of `font_set` (`0x32bb0`): glyph index limit and spacing stay
/// those of the sheet the font was *set* with, even when the menu code swaps
/// the sheet pointer directly (`0x51e460`) to draw with another one.
#[derive(Clone, Copy, Debug)]
pub struct FontState {
    pub first_char: i32,
    /// Sprite count of the sheet passed to `font_set` (`0x51e468`).
    pub limit: i32,
    pub spacing: i32,
}

impl FontState {
    pub fn set(sheet: &Sheet, first_char: i32, spacing: i32) -> Self {
        Self { first_char, limit: sheet.len() as i32, spacing }
    }

    /// Glyph index for a text byte, as both `0x32c10` and `0x32ce0` test it:
    /// `0 <= index <= limit` (one more than the sheet has; kept, but an index
    /// the drawing sheet does not have draws nothing here).
    fn glyph(&self, byte: u8) -> Option<usize> {
        let i = byte as i32 - self.first_char;
        (0..=self.limit).contains(&i).then_some(i as usize)
    }

    /// `text_measure` (`0x32c10`): sum of (low byte of glyph width + spacing).
    pub fn width(&self, sheet: &Sheet, text: &[u8]) -> i32 {
        text.iter().filter_map(|&b| self.glyph(b)).map(|i| (sheet.width(i) & 0xff) + self.spacing).sum()
    }
}

/// The drawing surfaces and the DAC.
pub struct Video {
    /// Back buffer (`0x4a5ae0`), pitch 640.
    pub back: Vec<u8>,
    /// What is on the VESA screen (only `pixels` is used; the colours come
    /// from [`Video::dac`]).
    pub screen: Framebuffer,
    /// Current DAC contents.
    pub dac: Pal,
    /// Sprite clip rows `y0`, `y1` of `sprite_set_target` (`0x33600`).
    pub clip_y0: i32,
    pub clip_y1: i32,
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Video {
    pub fn new() -> Self {
        Self { back: vec![0; W * H], screen: Framebuffer::new(), dac: BLACK, clip_y0: 0, clip_y1: H as i32 - 1 }
    }

    /// `palette_set(0, 255, pal)` (`0x331a0`).
    pub fn set_palette(&mut self, pal: &Pal) {
        self.dac = *pal;
    }

    /// `vesa_blit` of the whole back buffer (`0x32708`).
    pub fn present(&mut self) {
        self.screen.pixels.copy_from_slice(&self.back);
    }

    /// `vesa_blit(back, 0, 0, w, h, 0, 0, 640)`. The picture screens pass
    /// `0x27f, 0x1df`, one less than the size, so the last column and row of
    /// the screen keep what they had.
    pub fn present_size(&mut self, w: usize, h: usize) {
        for y in 0..h.min(H) {
            let o = y * W;
            self.screen.pixels[o..o + w.min(W)].copy_from_slice(&self.back[o..o + w.min(W)]);
        }
    }

    /// `vesa_blit` of the rectangle `x0..=x1` x `y0..=y1` at the same place.
    pub fn present_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (x0, x1) = (x0.clamp(0, W as i32 - 1) as usize, x1.clamp(0, W as i32 - 1) as usize);
        for y in y0.max(0)..=y1.min(H as i32 - 1) {
            let o = y as usize * W;
            self.screen.pixels[o + x0..=o + x1].copy_from_slice(&self.back[o + x0..=o + x1]);
        }
    }

    /// `vesa_fill_rect(0, 0, 639, 479, c)` (`0x32a04`).
    pub fn fill_screen(&mut self, c: u8) {
        self.screen.pixels.fill(c);
    }

    /// The back buffer as a drawing target with the current sprite clip.
    pub fn canvas(&mut self) -> Canvas<'_> {
        Canvas { pix: &mut self.back, clip_y0: self.clip_y0, clip_y1: self.clip_y1 }
    }

    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        self.canvas().fill_rect(x0, y0, x1, y1, c);
    }

    pub fn blit_picture(&mut self, pixels: &[u8], w: usize, h: usize) {
        self.canvas().blit_picture(pixels, w, h);
    }

    pub fn sprite(&mut self, sheet: &Sheet, index: usize, x: i32, y: i32) {
        self.canvas().sprite(sheet, index, x, y);
    }

    pub fn text(&mut self, sheet: &Sheet, font: &FontState, x: i32, y: i32, text: &[u8], align: Align) {
        self.canvas().text(sheet, font, x, y, text, align);
    }

    pub fn remap_box(&mut self, table: &[u8; 256], w: i32, h: i32) {
        self.canvas().remap_box(table, w, h);
    }

    pub fn remap_rect(&mut self, table: &[u8; 256], x0: i32, y0: i32, x1: i32, y1: i32) {
        self.canvas().remap_rect(table, x0, y0, x1, y1);
    }

    /// What the monitor shows: screen pixels through the DAC, 6-bit
    /// components widened by bit replication (verified against DOSBox-X
    /// captures, `docs/re/formats/flc.md`).
    pub fn show(&self, fb: &mut Framebuffer) {
        fb.pixels.copy_from_slice(&self.screen.pixels[..]);
        for (dst, src) in fb.palette.iter_mut().zip(&self.dac) {
            *dst = src.map(|c| {
                let c = c & 0x3f;
                (c << 2) | (c >> 4)
            });
        }
    }
}

/// A 640-pixel-wide drawing target (the game's back buffer `0x4a5ae0`, or
/// the island renderer's) with the sprite clip rows of `sprite_set_target`.
pub struct Canvas<'a> {
    pub pix: &'a mut [u8],
    pub clip_y0: i32,
    pub clip_y1: i32,
}

impl Canvas<'_> {
    /// `fill_rect(x0, y0, x1, y1, c)` on the back buffer (`0x310bb`), inclusive.
    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        let (x0, x1) = (x0.max(0), x1.min(W as i32 - 1));
        if x0 > x1 {
            return;
        }
        for y in y0.max(0)..=y1.min(H as i32 - 1) {
            let o = y as usize * W;
            self.pix[o + x0 as usize..=o + x1 as usize].fill(c);
        }
    }

    /// `rect_outline(x0, y0, x1, y1, c)` (`0x3102d`).
    pub fn rect_outline(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        self.fill_rect(x0, y0, x1, y0, c);
        self.fill_rect(x0, y1, x1, y1, c);
        self.fill_rect(x0, y0, x0, y1, c);
        self.fill_rect(x1, y0, x1, y1, c);
    }

    /// Copy a decoded picture (pitch = its width) to the back buffer at (0, 0).
    pub fn blit_picture(&mut self, pixels: &[u8], w: usize, h: usize) {
        if w == 0 {
            return;
        }
        for (y, row) in pixels.chunks_exact(w).take(h.min(H)).enumerate() {
            let n = w.min(W);
            self.pix[y * W..y * W + n].copy_from_slice(&row[..n]);
        }
    }

    /// `sprite_draw(index, x, y)` (`0x340e8`) on the back buffer.
    ///
    /// The top-left corner is `(x + hot_x, y + hot_y)`, but the row pointer is
    /// advanced by one line before each row is written (`0x3416b`), so row
    /// `r` lands on line `y + hot_y + r + 1`. Rows are clipped by comparing the
    /// linear offset of the *unadvanced* position with `clip_y0 * 640` and
    /// `clip_y1 * 640`; there is no horizontal clipping in the original (here
    /// pixels outside the buffer row are dropped instead of wrapping).
    pub fn sprite(&mut self, sheet: &Sheet, index: usize, x: i32, y: i32) {
        let Some(s) = sheet.sprites.get(index) else { return };
        let (px, py) = (x + s.hot_x, y + s.hot_y);
        let (top, bottom) = (self.clip_y0 * W as i32, self.clip_y1 * W as i32);
        for (r, runs) in s.rows.iter().enumerate() {
            let o = (py + r as i32) * W as i32 + px;
            if o < top {
                continue;
            }
            if o >= bottom {
                break;
            }
            let line = py + r as i32 + 1;
            if !(0..H as i32).contains(&line) {
                continue;
            }
            let base = line as usize * W;
            for (rx, pixels) in runs {
                let x0 = px + rx;
                for (i, &p) in pixels.iter().enumerate() {
                    let xx = x0 + i as i32;
                    if (0..W as i32).contains(&xx) {
                        self.pix[base + xx as usize] = p;
                    }
                }
            }
        }
    }

    /// `text_draw(x, y, text, align)` (`0x32ce0`): glyphs from `sheet`, limits
    /// and spacing from `font`. The pen advances by the full sprite width
    /// plus the spacing; the measured width uses the low byte of the width.
    pub fn text(&mut self, sheet: &Sheet, font: &FontState, x: i32, y: i32, text: &[u8], align: Align) {
        let width = font.width(sheet, text);
        let mut pen = match align {
            Align::Left => x,
            Align::Centre => x - width / 2,
            Align::Right => x - width,
        };
        for &b in text {
            if let Some(i) = font.glyph(b) {
                self.sprite(sheet, i, pen, y);
                pen += sheet.width(i) + font.spacing;
            }
        }
    }

    /// `0x253d0`: remap a centred `w` x `h` box of the back buffer through
    /// `table`. Rows `1..h` of the box are touched (the loop skips row 0).
    pub fn remap_box(&mut self, table: &[u8; 256], w: i32, h: i32) {
        let x0 = 0x140 - w / 2;
        let y0 = 0xf0 - h / 2;
        for row in 1..h {
            let y = y0 + row;
            if !(0..H as i32).contains(&y) {
                continue;
            }
            for x in x0..x0 + w {
                if (0..W as i32).contains(&x) {
                    let p = &mut self.pix[y as usize * W + x as usize];
                    *p = table[*p as usize];
                }
            }
        }
    }

    /// `0x10110` of `THE_END.EXE`: remap `x0..=x1` x `y0..=y1` through `table`.
    pub fn remap_rect(&mut self, table: &[u8; 256], x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0.max(0)..=y1.min(H as i32 - 1) {
            for x in x0.max(0)..=x1.min(W as i32 - 1) {
                let p = &mut self.pix[y as usize * W + x as usize];
                *p = table[*p as usize];
            }
        }
    }
}

/// `0x252d0`: for every colour, the palette index nearest to that colour
/// moved `amount / 32` of the way towards `(r, g, b)`. The menu box uses
/// black and 16 (half brightness). Ties go to the highest index (`<=`).
pub fn blend_table(target: [i32; 3], amount: i32, pal: &Pal) -> [u8; 256] {
    let mut table = [0u8; 256];
    for (i, c) in pal.iter().enumerate() {
        // Signed division by 32 truncating toward zero (cdq / shl edx,5 / sbb / sar 5).
        let want: [i32; 3] = std::array::from_fn(|k| c[k] as i32 + (target[k] - c[k] as i32) * amount / 32);
        let mut best = 0xb98c0;
        let mut best_i = 0;
        for (j, d) in pal.iter().enumerate() {
            let dist: i32 = (0..3).map(|k| (d[k] as i32 - want[k]).pow(2)).sum();
            if dist <= best {
                best = dist;
                best_i = j;
            }
        }
        table[i] = best_i as u8;
    }
    table
}

/// One colour of `palette_fade` (`0x331f0`) at step `i` of `steps`:
/// `from + (to - from) * i / steps`, truncating.
fn fade_component(from: u8, to: u8, i: i32, steps: i32) -> u8 {
    (from as i32 + (to as i32 - from as i32) * i / steps) as u8
}

/// `palette_fade(first, 255, steps, from, to)` (`0x331f0`) as state: the
/// original runs `steps + 1` iterations, each after one vertical retrace.
#[derive(Clone)]
pub struct Fade {
    first: usize,
    steps: i32,
    i: i32,
    from: Box<Pal>,
    to: Box<Pal>,
}

impl Fade {
    pub fn new(first: usize, steps: i32, from: &Pal, to: &Pal) -> Self {
        Self { first, steps, i: 0, from: Box::new(*from), to: Box::new(*to) }
    }

    /// One retrace: program the next step. Returns `true` when that was the last.
    pub fn step(&mut self, dac: &mut Pal) -> bool {
        if self.i > self.steps {
            return true;
        }
        if self.steps > 0 {
            for c in self.first..256 {
                for k in 0..3 {
                    dac[c][k] = fade_component(self.from[c][k], self.to[c][k], self.i, self.steps);
                }
            }
        } else {
            // steps == 0: one iteration dividing by zero in the original; never called that way.
            dac[self.first..].copy_from_slice(&self.to[self.first..]);
        }
        self.i += 1;
        self.i > self.steps
    }

    /// Retraces the whole fade takes.
    pub fn len(steps: i32) -> u32 {
        (steps + 1).max(0) as u32
    }
}

/// The red flash palette of the cheat confirmation (`0x25dc0`, also
/// `0x26f40` in-game): red = average of the components, green = blue = 0.
pub fn red_palette(pal: &Pal) -> Pal {
    let mut out = BLACK;
    for (o, c) in out.iter_mut().zip(pal) {
        o[0] = ((c[0] as u32 + c[1] as u32 + c[2] as u32) / 3) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_runs_steps_plus_one_and_ends_exact() {
        let from = BLACK;
        let mut to = BLACK;
        to[5] = [63, 10, 1];
        let mut dac = BLACK;
        let mut f = Fade::new(0, 16, &from, &to);
        let mut n = 0;
        loop {
            n += 1;
            let done = f.step(&mut dac);
            if n == 1 {
                assert_eq!(dac[5], [0, 0, 0]);
            }
            if n == 9 {
                assert_eq!(dac[5], [31, 5, 0]);
            }
            if done {
                break;
            }
        }
        assert_eq!(n, 17);
        assert_eq!(dac[5], [63, 10, 1]);
    }

    #[test]
    fn blend_table_halves_towards_black() {
        let mut pal = BLACK;
        pal[1] = [40, 40, 40];
        pal[2] = [20, 20, 20];
        pal[3] = [20, 20, 20];
        let t = blend_table([0, 0, 0], 16, &pal);
        // 40 -> 20; two equal candidates: the higher index wins.
        assert_eq!(t[1], 3);
    }
}
