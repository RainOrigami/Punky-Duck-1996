# `.MOV` — Autodesk Animator Pro FLC

Decoder: `crates/pd-formats/src/flc.rs` (written from the published FLC format
description, not from the disassembly). Player: `crates/pd-engine/src/movie.rs`.
Test over the real files: `all_movies_decode` in `crates/pd-engine/tests/real_disc.rs`.

## Files — **verified** (all frames of all five decode; header values asserted in the test)

| File | Frames | Speed | Duration | Sub-chunks used |
|---|---|---|---|---|
| `C_SOFT.MOV` | 101 | 100 ms | 10.1 s | 4, 15, 7 ×101, 18 |
| `INTRO.MOV` | 560 | 114 ms | 63.8 s | 4 ×12, 15, 7 ×351, 18; 209 empty frames |
| `PART_1.MOV` | 12 | 114 ms | 1.4 s | 4, 15, 7 ×12, 18 |
| `PART_2.MOV` | 71 | 114 ms | 8.1 s | 4, 15, 7 ×58, 18; 13 empty frames |
| `PUNKY.MOV` | 20 | 71 ms | 1.4 s | 4, 15, 7 ×20, 18 |

All are 640×480, 8 bits, flags `0x0003`. Each file has one prefix chunk (`0xF100`)
before the first frame and one ring frame after the last (frame chunks = frames + 1;
counts above include the ring frame). The per-frame delay field is 0 in every frame chunk.

## Header (128 bytes, little-endian)

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | file size |
| 4 | u16 | magic `0xAF12` |
| 6 | u16 | frame count (without the ring frame) |
| 8, 10 | u16 | width, height |
| 12 | u16 | depth (8) |
| 14 | u16 | flags |
| 16 | u32 | speed: milliseconds per frame |
| 80 | u32 | offset of frame 1 |
| 84 | u32 | offset of frame 2 (loop target after the ring frame) |

## Chunks

Top level: `u32 size, u16 type`. `0xF100` prefix (skipped), `0xF1FA` frame:
`u16 sub-chunk count` at +6, `u16 delay` at +8 (ms, 0 = use header speed), data at +16.
Sub-chunks: `u32 size, u16 type`, body at +6.

| Type | Name | Decoding | Status |
|---|---|---|---|
| 4 | COLOR_256 | `u16 packets`; each `u8 skip, u8 count (0 = 256)`, `count` RGB triples, 8 bits per channel | **verified** |
| 7 | DELTA_FLC | `u16 lines`; per line `u16` opcodes: top bits `00` packet count, `11` skip −value lines, `10` set last pixel of the line; packets `u8 skip, i8 n`: `n ≥ 0` copy `n` words, `n < 0` repeat one word `−n` times | **verified** |
| 15 | BYTE_RUN | per line: 1 ignored count byte, then packets until the line is full: `i8 n`; `n > 0` repeat next byte `n` times, `n < 0` copy `−n` bytes | **verified** |
| 18 | PSTAMP | thumbnail, ignored | **verified** (ignoring it gives correct pictures) |
| 13 | BLACK | clear to 0 | implemented, **untested** (does not occur) |
| 16 | FLI_COPY | `width × height` raw bytes | implemented, **untested** (does not occur) |
| other | | decoding stops with an error (`unsupported chunk type`) | by design |

Basis for "verified":

- Decoded pictures of all five files were viewed (first, middle, last frame): publisher
  logo, space scene, island scenes, "THE END", title screen; no corruption.
- `C_SOFT.MOV` and `INTRO.MOV` were compared with a lossless DOSBox-X capture of the
  original `INTRO.EXE` (`tools/dosbox/run.sh --headless --limit 110 --capture INTRO.EXE`;
  copy in `out/reference/`). Every decoded frame was hashed after the palette mapping
  below and looked up among the capture's pictures: 418 of the capture's 951 distinct
  consecutive pictures are byte-identical to a decoded frame, among them every frame
  whose picture is unique within its movie that was looked for (66 in C_SOFT frames
  0–69, 128 in INTRO frames 0–546). Three capture pictures inside C_SOFT that the hash
  pass had not matched were compared directly and are identical to frames 34, 53 and 56
  too, so the hash pass undercounts (reason not investigated). The other unmatched
  pictures were not examined one by one; they include the fades between and after the
  movies. No capture picture was found that contradicts the decoder.

## Palette as displayed — **verified** against the capture

The FLC palettes use all 8 bits per channel, the VGA DAC has 6. The capture equals the
decoded frames exactly when each component is mapped `c6 = v >> 2`, shown as
`(c6 << 2) | (c6 >> 4)`. With `v` unchanged, `(v >> 2) << 2` or `c6 * 255 / 63` the
frames do not match (24–99 % of pixels equal, depending on the frame).
`pd_engine::movie::vga_dac` applies the mapping when a movie is rendered. The
`(c6 << 2) | (c6 >> 4)` widening is how DOSBox-X presents a 6-bit DAC value.

## Playback timing in the original — measured from the capture

The capture runs at 70.007 Hz (the video mode's refresh). Frame changes land on retraces.

- `C_SOFT.MOV` (100 ms): a new frame every 7 retraces (= 99.99 ms) over frames 5–69,
  with a single one-retrace slip. **verified**
- `INTRO.MOV` (114 ms): mostly every 8 retraces with an occasional 7; over frames 5–399
  the average is 7.980 retraces = 113.98 ms per frame. That is the header value, so
  frames are scheduled in milliseconds and shown on the next retrace rather than by a
  fixed retrace count. **verified** for that span.
- The first frames appear late and are then caught up (C_SOFT frame 0 is on screen 33
  retraces after the schedule extrapolated from frames 5–69 says, frames 1–4 pass within
  2 retraces). Looks like a schedule anchored before the file was loaded; the amount
  depends on the emulated CPU speed. **hypothesis**
- Between INTRO frames 399 and 530 the capture gets 28 retraces ahead of the 114 ms
  schedule (about 111 ms per frame in that span). **open** (long runs of repeated
  pictures there make single frames unidentifiable in the capture).
- CD music: track 2 starts with `C_SOFT.MOV`, track 3 with `INTRO.MOV` (DOSBox-X log).
  **verified**

`MoviePlayer` advances by the header's millisecond value in real time (frame `k` is due
at `k × speed`), independent of the host refresh rate. It does not reproduce the initial
catch-up or the unexplained speed-up. Measured in the native frontend: `C_SOFT.MOV` runs
10.10 s on a 60 Hz and on a 165 Hz monitor.
