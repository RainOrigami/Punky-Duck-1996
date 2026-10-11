//! Autodesk Animator Pro FLC decoder (the game's `.MOV` files).
//!
//! Spec: `docs/re/formats/flc.md`. Streaming: one call to
//! [`Flc::decode_next`] applies one frame chunk to the indexed picture and the
//! palette. Chunk types that do not occur in the five movies on the disc and
//! are not trivially specified are rejected with an error instead of guessed.

use std::fmt;

pub const FLC_MAGIC: u16 = 0xAF12;
const HEADER_SIZE: usize = 128;
const FRAME_MAGIC: u16 = 0xF1FA;
const PREFIX_MAGIC: u16 = 0xF100;

const CHUNK_COLOR_256: u16 = 4;
const CHUNK_DELTA_FLC: u16 = 7;
const CHUNK_BLACK: u16 = 13;
const CHUNK_BYTE_RUN: u16 = 15;
const CHUNK_FLI_COPY: u16 = 16;
const CHUNK_PSTAMP: u16 = 18;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlcError(pub String);

impl fmt::Display for FlcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FLC: {}", self.0)
    }
}

impl std::error::Error for FlcError {}

fn err<T>(msg: impl Into<String>) -> Result<T, FlcError> {
    Err(FlcError(msg.into()))
}

/// Fields of the 128-byte file header that matter for playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlcHeader {
    /// Size of the whole file in bytes.
    pub size: u32,
    /// Number of frames, not counting the trailing ring frame.
    pub frames: u16,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub flags: u16,
    /// Delay between frames in milliseconds.
    pub speed_ms: u32,
    /// File offset of the first frame chunk (0 if the creator did not set it).
    pub oframe1: u32,
    /// File offset of the second frame chunk (loop target after the ring frame).
    pub oframe2: u32,
}

/// What a decoded frame changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameInfo {
    /// Zero-based index of the frame that was just decoded.
    pub index: u32,
    /// How long this frame stays on screen, in milliseconds.
    pub delay_ms: u32,
    pub palette_changed: bool,
    pub pixels_changed: bool,
}

/// Bounds-checked little-endian cursor.
struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FlcError> {
        if self.d.len() - self.p < n {
            return err("chunk data truncated");
        }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, FlcError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FlcError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// Streaming FLC decoder over any byte container (`Vec<u8>`, `&[u8]`, ...).
pub struct Flc<D: AsRef<[u8]>> {
    data: D,
    header: FlcHeader,
    /// File offset of the next top-level chunk.
    pos: usize,
    /// Frames decoded since the start / last rewind.
    next_frame: u32,
    pixels: Vec<u8>,
    palette: [[u8; 3]; 256],
}

impl<D: AsRef<[u8]>> Flc<D> {
    pub fn new(data: D) -> Result<Self, FlcError> {
        let d = data.as_ref();
        if d.len() < HEADER_SIZE {
            return err("file shorter than the 128-byte header");
        }
        let magic = u16_at(d, 4);
        if magic != FLC_MAGIC {
            return err(format!("bad magic {magic:#06x} (want 0xaf12)"));
        }
        let header = FlcHeader {
            size: u32_at(d, 0),
            frames: u16_at(d, 6),
            width: u16_at(d, 8),
            height: u16_at(d, 10),
            depth: u16_at(d, 12),
            flags: u16_at(d, 14),
            speed_ms: u32_at(d, 16),
            oframe1: u32_at(d, 80),
            oframe2: u32_at(d, 84),
        };
        if header.depth != 8 {
            return err(format!("unsupported colour depth {}", header.depth));
        }
        if header.width == 0 || header.height == 0 {
            return err("zero-sized picture");
        }
        let pixels = vec![0u8; header.width as usize * header.height as usize];
        Ok(Flc { data, header, pos: HEADER_SIZE, next_frame: 0, pixels, palette: [[0; 3]; 256] })
    }

    pub fn header(&self) -> &FlcHeader {
        &self.header
    }

    pub fn width(&self) -> usize {
        self.header.width as usize
    }

    pub fn height(&self) -> usize {
        self.header.height as usize
    }

    /// Indexed picture, `width * height` bytes, row-major from the top.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// 256 RGB entries, 8 bits per channel.
    pub fn palette(&self) -> &[[u8; 3]; 256] {
        &self.palette
    }

    /// Number of frames decoded so far (since the last rewind).
    pub fn frames_decoded(&self) -> u32 {
        self.next_frame
    }

    /// Restart from the first frame. The picture and palette are cleared.
    pub fn rewind(&mut self) {
        self.pos = HEADER_SIZE;
        self.next_frame = 0;
        self.pixels.fill(0);
        self.palette = [[0; 3]; 256];
    }

    /// Decode the next frame into the picture/palette. Returns `Ok(None)` once
    /// all `header.frames` frames were decoded (the ring frame that transforms
    /// the last frame back into the first is not played).
    pub fn decode_next(&mut self) -> Result<Option<FrameInfo>, FlcError> {
        if self.next_frame >= self.header.frames as u32 {
            return Ok(None);
        }
        let d = self.data.as_ref();
        loop {
            if d.len() - self.pos.min(d.len()) < 6 {
                return err(format!("file ends before frame {}", self.next_frame));
            }
            let size = u32_at(d, self.pos) as usize;
            let kind = u16_at(d, self.pos + 4);
            if size < 6 || size > d.len() - self.pos {
                return err(format!("chunk at {:#x} has bad size {size}", self.pos));
            }
            let chunk = &d[self.pos..self.pos + size];
            match kind {
                // Prefix chunk (Animator Pro settings, postage stamp): no picture data.
                PREFIX_MAGIC => self.pos += size,
                FRAME_MAGIC => {
                    if size < 16 {
                        return err("frame chunk shorter than its header");
                    }
                    let subchunks = u16_at(chunk, 6);
                    let delay = u16_at(chunk, 8);
                    let mut info = FrameInfo {
                        index: self.next_frame,
                        delay_ms: if delay != 0 { delay as u32 } else { self.header.speed_ms },
                        ..FrameInfo::default()
                    };
                    let mut p = 16;
                    for _ in 0..subchunks {
                        if chunk.len() - p < 6 {
                            return err("sub-chunk header truncated");
                        }
                        let sub_size = u32_at(chunk, p) as usize;
                        let sub_kind = u16_at(chunk, p + 4);
                        if sub_size < 6 || sub_size > chunk.len() - p {
                            return err(format!("sub-chunk type {sub_kind} has bad size {sub_size}"));
                        }
                        let body = &chunk[p + 6..p + sub_size];
                        let (w, h) = (self.header.width as usize, self.header.height as usize);
                        match sub_kind {
                            CHUNK_COLOR_256 => {
                                decode_color_256(body, &mut self.palette)?;
                                info.palette_changed = true;
                            }
                            CHUNK_DELTA_FLC => {
                                decode_delta_flc(body, &mut self.pixels, w, h)?;
                                info.pixels_changed = true;
                            }
                            CHUNK_BYTE_RUN => {
                                decode_byte_run(body, &mut self.pixels, w, h)?;
                                info.pixels_changed = true;
                            }
                            CHUNK_BLACK => {
                                self.pixels.fill(0);
                                info.pixels_changed = true;
                            }
                            CHUNK_FLI_COPY => {
                                if body.len() < w * h {
                                    return err("FLI_COPY chunk truncated");
                                }
                                self.pixels.copy_from_slice(&body[..w * h]);
                                info.pixels_changed = true;
                            }
                            // Postage stamp (thumbnail of the first frame): not part of the picture.
                            CHUNK_PSTAMP => {}
                            other => {
                                return err(format!(
                                    "unsupported chunk type {other} in frame {}",
                                    self.next_frame
                                ));
                            }
                        }
                        p += sub_size;
                    }
                    self.pos += size;
                    self.next_frame += 1;
                    return Ok(Some(info));
                }
                other => {
                    return err(format!("unknown top-level chunk {other:#06x} at {:#x}", self.pos));
                }
            }
        }
    }
}

/// COLOR_256 (type 4): `u16 packets`; each `u8 skip, u8 count (0 = 256)`, then
/// `count` RGB triples with 8 bits per channel.
fn decode_color_256(body: &[u8], palette: &mut [[u8; 3]; 256]) -> Result<(), FlcError> {
    let mut r = Rd { d: body, p: 0 };
    let packets = r.u16()?;
    let mut index = 0usize;
    for _ in 0..packets {
        index += r.u8()? as usize;
        let mut count = r.u8()? as usize;
        if count == 0 {
            count = 256;
        }
        if index + count > 256 {
            return err("palette packet runs past entry 255");
        }
        for rgb in r.take(count * 3)?.chunks_exact(3) {
            palette[index] = [rgb[0], rgb[1], rgb[2]];
            index += 1;
        }
    }
    Ok(())
}

/// BYTE_RUN (type 15): full picture, each line RLE-compressed. A line starts
/// with a packet-count byte that FLC decoders must ignore (it overflows for
/// wide pictures); packets are decoded until the line is full. Packet: `i8 n`;
/// `n > 0` repeats the next byte `n` times, `n < 0` copies `-n` literal bytes.
fn decode_byte_run(body: &[u8], pixels: &mut [u8], w: usize, h: usize) -> Result<(), FlcError> {
    let mut r = Rd { d: body, p: 0 };
    for line in pixels.chunks_exact_mut(w).take(h) {
        r.u8()?; // obsolete packet count
        let mut x = 0;
        while x < w {
            let n = r.u8()? as i8;
            let len = n.unsigned_abs() as usize;
            if len == 0 || x + len > w {
                return err("BYTE_RUN packet overruns the line");
            }
            if n > 0 {
                let v = r.u8()?;
                line[x..x + len].fill(v);
            } else {
                line[x..x + len].copy_from_slice(r.take(len)?);
            }
            x += len;
        }
    }
    Ok(())
}

/// DELTA_FLC (type 7, "SS2"): word-oriented delta against the previous frame.
///
/// `u16 lines`; for each encoded line a sequence of `u16` opcodes selected by
/// the top two bits: `00` = packet count (ends the opcode sequence), `11` =
/// skip `-(i16)` lines, `10` = low byte is the value of the last pixel of the
/// line (odd widths). Packets: `u8 skip, i8 n`; `n >= 0` copies `n` literal
/// words, `n < 0` repeats the next word `-n` times.
fn decode_delta_flc(body: &[u8], pixels: &mut [u8], w: usize, h: usize) -> Result<(), FlcError> {
    let mut r = Rd { d: body, p: 0 };
    let lines = r.u16()?;
    let mut y = 0usize;
    for _ in 0..lines {
        let packets = loop {
            let op = r.u16()?;
            match op >> 14 {
                0b00 => break op,
                0b11 => y += (op as i16).unsigned_abs() as usize,
                0b10 => {
                    if y >= h {
                        return err("DELTA_FLC last-pixel opcode below the picture");
                    }
                    pixels[y * w + w - 1] = op as u8;
                }
                _ => return err("DELTA_FLC opcode with undefined type bits 01"),
            }
        };
        if packets > 0 && y >= h {
            return err("DELTA_FLC line below the picture");
        }
        let mut x = 0usize;
        for _ in 0..packets {
            x += r.u8()? as usize;
            let n = r.u8()? as i8;
            let len = n.unsigned_abs() as usize * 2;
            if x + len > w {
                return err("DELTA_FLC packet overruns the line");
            }
            let dst = &mut pixels[y * w + x..y * w + x + len];
            if n >= 0 {
                dst.copy_from_slice(r.take(len)?);
            } else {
                let word = r.take(2)?;
                for pair in dst.chunks_exact_mut(2) {
                    pair.copy_from_slice(word);
                }
            }
            x += len;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: u16, body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 6) as u32).to_le_bytes().to_vec();
        v.extend_from_slice(&kind.to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    fn frame(subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut v = ((body.len() + 16) as u32).to_le_bytes().to_vec();
        v.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
        v.extend_from_slice(&(subs.len() as u16).to_le_bytes());
        v.extend_from_slice(&[0; 8]);
        v.extend_from_slice(&body);
        v
    }

    fn file(w: u16, h: u16, speed: u32, frames: &[Vec<u8>]) -> Vec<u8> {
        let mut v = vec![0u8; HEADER_SIZE];
        v[4..6].copy_from_slice(&FLC_MAGIC.to_le_bytes());
        v[6..8].copy_from_slice(&(frames.len() as u16).to_le_bytes());
        v[8..10].copy_from_slice(&w.to_le_bytes());
        v[10..12].copy_from_slice(&h.to_le_bytes());
        v[12..14].copy_from_slice(&8u16.to_le_bytes());
        v[16..20].copy_from_slice(&speed.to_le_bytes());
        for f in frames {
            v.extend_from_slice(f);
        }
        let len = v.len() as u32;
        v[0..4].copy_from_slice(&len.to_le_bytes());
        v
    }

    #[test]
    fn synthetic_movie() {
        // 4x2 picture. Frame 0: palette + BYTE_RUN; frame 1: DELTA_FLC; frame 2: empty.
        let pal = chunk(CHUNK_COLOR_256, &[1, 0, 1, 2, 10, 20, 30, 40, 50, 60]);
        let brun = chunk(
            CHUNK_BYTE_RUN,
            &[
                1, 4, 7, // line 0: 4 x 7
                2, 0xfe, 1, 2, 2, 9, // line 1: literal 1,2 then 2 x 9
            ],
        );
        let delta = chunk(
            CHUNK_DELTA_FLC,
            &[
                1, 0, // one encoded line
                0xff, 0xff, // skip 1 line
                2, 0, // two packets
                0, 1, 5, 6, // skip 0, copy one word (5,6)
                0, 0xff, 3, 4, // skip 0, repeat word (3,4) once
            ],
        );
        let data = file(4, 2, 70, &[frame(&[pal, brun]), frame(&[delta]), frame(&[])]);
        let mut flc = Flc::new(&data[..]).unwrap();
        assert_eq!(flc.header().frames, 3);

        let f0 = flc.decode_next().unwrap().unwrap();
        assert!(f0.palette_changed && f0.pixels_changed);
        assert_eq!(f0.delay_ms, 70);
        assert_eq!(flc.pixels(), &[7, 7, 7, 7, 1, 2, 9, 9]);
        assert_eq!(flc.palette()[0], [0, 0, 0]);
        assert_eq!(flc.palette()[1], [10, 20, 30]);
        assert_eq!(flc.palette()[2], [40, 50, 60]);

        let f1 = flc.decode_next().unwrap().unwrap();
        assert!(!f1.palette_changed && f1.pixels_changed);
        assert_eq!(flc.pixels(), &[7, 7, 7, 7, 5, 6, 3, 4]);

        let f2 = flc.decode_next().unwrap().unwrap();
        assert_eq!((f2.index, f2.palette_changed, f2.pixels_changed), (2, false, false));
        assert_eq!(flc.decode_next().unwrap(), None);

        flc.rewind();
        assert_eq!(flc.decode_next().unwrap().unwrap().index, 0);
    }

    #[test]
    fn unknown_chunk_is_an_error() {
        let data = file(4, 2, 70, &[frame(&[chunk(12, &[0, 0, 0, 0])])]);
        let mut flc = Flc::new(&data[..]).unwrap();
        assert!(flc.decode_next().is_err());
    }

    #[test]
    fn truncated_data_is_an_error() {
        let data = file(4, 2, 70, &[frame(&[chunk(CHUNK_BYTE_RUN, &[1, 4])])]);
        let mut flc = Flc::new(&data[..]).unwrap();
        assert!(flc.decode_next().is_err());
        assert!(Flc::new(&[0u8; 10][..]).is_err());
    }
}
