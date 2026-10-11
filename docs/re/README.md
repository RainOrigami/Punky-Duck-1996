# Reverse-engineering notes

Status legend: **verified** = decoded output checked or read from disassembly (cite address);
**hypothesis** = inferred from sample bytes only.

One file per topic; add to the index when creating one.

- `README.md` — this overview
- `functions.md` — function map of `P.EXE`
- `le2elf.md` — LE→ELF conversion, address layout of `P.elf`
- `engine/timing.md`, `engine/states.md`, `engine/renderer.md`, `engine/sound.md`, `engine/input.md`, `engine/menus.md` (menus, game over, modal screens, `INTRO.EXE`, `THE_END.EXE`) — engine write-ups
- `formats/*.md` — one spec per format
- `game/*.md` — gameplay specs: `overview`, `player`, `entities`, `items-and-scripts`, `sections`, `save-and-settings`, `globals` (name-to-address table), `open-questions`
- `reference.md` — running the original in DOSBox-X, observations

## Disc

`disc/PUNKYDUCK.cue`: track 1 MODE2/2352 (ISO9660, 16051 sectors, user data at sector
offset 24, 2048 bytes), tracks 2–9 Red Book audio (music). Volume `PUNKY DUCK`,
publisher `CONDOR SOFTWARE`, files dated 1996-09. **verified**

## Executables

All Watcom C/C++32 10.x, DOS/4GW bound, LE format. No symbols.

- `P.EXE` — main game. LE header at file offset `0x2c80`, page size 4096, data pages at `0x19200` (relative to file start per LE convention — check), entry object 1 + `0x23db8`. **verified** (header fields)

  | Obj | Base | Virtual size | Flags | Pages |
  |---|---|---|---|---|
  | 1 (code) | `0x10000` | `0x2df0f` | `0x2045` | 46 |
  | 2 (data/bss) | `0x40000` | `0x4e0bc0` | `0x2043` | 4 |
  | 3 | `0x530000` | `0x37ec` | `0x2043` | 2 |
  | 4 | `0x540000` | `0x808` | `0x2043` | 1 |

- `INTRO.EXE`, `THE_END.EXE` — intro / ending players (`INTRO.INI` holds credit pages).
- `INST.EXE`, `SETUP/SETD.EXE` — installer and sound setup; not needed.
- `DUCK.BAT` runs `intro`, `P.exe`, `the_end.exe` in sequence.

P.EXE contains its own sound mixer (8/16-bit, mono/stereo, DMA strings), a crash handler
that looks up a `.map` file, joystick calibration, and CD detection (`CD.ID`,
`CD Detected... %d`). It references `DEMOILE0.DAT`, `ILE_0.TXT`, `BOSS0.SPT`, `GO_0.RON`
which are not on the disc (name templates with a digit patched in — hypothesis).

## Formats

| Ext | Content | Status |
|---|---|---|
| `.BNK` | Archive. `u32 count`; `count` × { `char name[12]` NUL-padded, `u32 offset`, `u32 size` }; offsets absolute. `BANQUE.BNK` 30 × SPT, `PICTURE.BNK` 54 × RON, `SAMPLE.BNK` 29 × PCM. | **verified** |
| `.RON` | Image = GIF with every byte XOR `0x55` and signature `RON` instead of `GIF`; standard LZW. Palette 8-bit in file. See `formats/ron.md`. | **verified** |
| `.MOV` | Autodesk FLC, 640×480×8; chunk types 4, 7, 15, 18 occur. Palette shown through the 6-bit VGA DAC. See `formats/flc.md`; disc image layout in `formats/disc.md`. | **verified** against DOSBox capture |
| `.SPT` | Sprites, uncompressed run lists per row; entry offsets relative to file offset 4; trailing 768-byte palette appears unused. Palette comes from `ILE_n.COL` / current RON. See `formats/spt.md`. | **verified** |
| `.PCM` | Raw signed 16-bit LE mono, 16000 Hz. See `formats/pcm.md`. | **verified** from mixer code, not by ear |
| `.UNI` | Island terrain, uncompressed: palette, 32 shade (fog) tables, 32x32 tile sheet, 256x256 cell map (tile + rotation), attribute map, height map. Not a picture. See `formats/uni.md`. | **verified** |
| `.MAP` | Texture page for 3D objects with a rectangle table per texture. See `formats/map.md`. | **verified** |
| `.3D` | Meshes: per-object table, `f32` points, 24-byte textured triangle faces. Placement is hard-coded per island in P.EXE. See `formats/3d.md`. | **verified** |
| `.TRJ` | Spawn lists with optional paths: `ILE0n` = bonuses, `MONSTERn` = monsters. See `formats/trj.md`. | layout **verified**; monster path following open |
| `.COL` | In-game island palette, 256 x 8-bit RGB after an ignored 8-byte header. See `formats/col.md`. | **verified** |
| `ILE_n.TXT` | Dialogue. Id line, then 5 language lines (FR, EN, ES, DE, PL) of 4 quoted strings. One custom character set (CP437 + own Polish block). See `formats/txt.md`. | **verified** |
| `GAME.STP` | 8 x `u32`: sound on, music on, controller, SFX volume, CD volume, difficulty, language, game speed. See `formats/stp.md`. | **verified** |
| `INSTALL.STP` | Text file list for the installer. | **verified** |
