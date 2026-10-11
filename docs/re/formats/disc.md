# Disc image (`PUNKYDUCK.bin` / `.cue`)

Reader: `crates/pd-formats/src/disc.rs`. Tests against the real image:
`crates/pd-engine/tests/real_disc.rs`.

## Layout — **verified** (parsed image, test `audio_tracks_match_cue`)

One raw file, 56446 sectors of 2352 bytes (132 760 992 bytes), described by a single-`FILE`
cue sheet. Positions are `mm:ss:ff` at 75 frames (sectors) per second from the file start.

| Track | Mode | INDEX 01 | First sector | Sectors | Length |
|---|---|---|---|---|---|
| 1 | MODE2/2352 | 00:00:00 | 0 | 16051 | data |
| 2 | AUDIO | 03:34:01 | 16051 | 1143 | 15.24 s |
| 3 | AUDIO | 03:49:19 | 17194 | 5189 | 69.19 s |
| 4 | AUDIO | 04:58:33 | 22383 | 12297 | 163.96 s |
| 5 | AUDIO | 07:42:30 | 34680 | 9562 | 127.49 s |
| 6 | AUDIO | 09:49:67 | 44242 | 452 | 6.03 s |
| 7 | AUDIO | 09:55:69 | 44694 | 2805 | 37.40 s |
| 8 | AUDIO | 10:33:24 | 47499 | 7804 | 104.05 s |
| 9 | AUDIO | 12:17:28 | 55303 | 1143 | 15.24 s |

A track ends where the next one's `INDEX 00` (if present, else `INDEX 01`) starts; the
last one ends with the file. The cue has no `INDEX 00` entries, so each audio track's
slice includes the following track's pregap: every audio track starts with 1 all-zero
sector and ends with 150–151 all-zero sectors (2 s of silence). **verified** (counted)

No sector of tracks 2–9 starts with the CD-ROM sync pattern, i.e. they are all audio.
**verified**

## Data track — **verified**

- Sector = 12 bytes sync (`00 FF×10 00`), 4 bytes header (BCD minute, second, frame,
  mode = 2), 8 bytes XA subheader (`00 00 08 00` twice = form 1 data), **2048 bytes
  user data at offset 24**, then EDC/ECC (ignored). Sector 0 header: `00 02 00 02`.
- ISO9660, logical block size 2048, block `n` = sector `n` of the track. Primary volume
  descriptor in block 16: volume id `PUNKY DUCK`, volume space 15674 blocks (the rest of
  the track is zero padding).
- Directory records as in ECMA-119: `[0]` length, `[2]` extent LBA (LE u32), `[10]` size
  (LE u32), `[25]` flags (bit 1 = directory), `[32]` name length, `[33]` name; names carry
  a `;1` version suffix; a zero length byte pads to the next block.
- The root directory holds exactly one entry, the directory **`PUNKY`**, which holds the
  105 files (including those of its sub-directory `SETUP`). `disc/extracted/PUNKY/` is
  that directory. The game addresses files by bare name (`BANQUE.BNK`), so
  `pd_engine::assets::DiscAssets` resolves names relative to the directory containing
  `P.EXE`.
- Every file read through the reader is byte-identical to its copy in
  `disc/extracted/PUNKY/` (test `filesystem_matches_extracted_directory`, all 105 files).

## Audio tracks — **verified**

Red Book: 44100 Hz, stereo, signed 16-bit little-endian, interleaved L/R, 588 sample
frames per sector, no header. Exposed as `AudioTrack` (borrowed bytes with `frame(i)`,
`read(start, out)`, `to_vec()`; not a `&[i16]` because the bytes are not guaranteed to be
aligned).

Which track plays where is game logic (`docs/re/engine/sound.md`, `docs/re/reference.md`).
The DOSBox-X log of `INTRO.EXE` shows `Playing track # 2`, then `# 3`. **verified** by
that log.

## Reader limits (by design, they fail with an error)

Several `FILE`s, non-`BINARY` files, track modes other than `AUDIO`, `MODE1/2352`,
`MODE2/2352`, a bin size that is not a multiple of 2352, unknown cue keywords.
`MODE1/2352` (user data at offset 16) is accepted but **untested** (no such image here).
