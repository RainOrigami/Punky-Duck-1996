# Function map of P.EXE (addresses as in `disc/extracted/P.elf`)

See `le2elf.md` for how the ELF is produced. All addresses are link-time addresses.
Names are proposed (no symbols exist). Confidence: **V** = verified by reading the
disassembly/decompilation of that function, **H** = hypothesis (inferred from callers,
strings or position only).

Calling convention of the game's C code: Watcom stack convention (args pushed right to
left, caller cleans, result in EAX, floats in ST0). The hand-written asm library
(blitters etc.) uses the same stack convention with `pusha`/`popa` (first arg at
`[esp+0x24]`).

## Code layout — **V**

| Range | Content |
|---|---|
| `0x10000`–`0x100be` | debugger/int3 stub, padding |
| `0x100be`–`0x2b370` | game code (C): 3D/landscape engine, per-island modules, gameplay, menus, HUD, shop |
| `0x2b370`–`0x2c838` | game support: hot zones, BNK/RON loader, FLC player |
| `0x2c838`–`0x2fa30` | sound: mixer inner loops, driver loader (`SOUNDRV.COM`/`VECTOR.COM`), sample bank, DMA/IRQ |
| `0x2fa30`–`0x30570` | crash handler (`PROCESSOR EXCEPTION`, `.MAP` lookup) |
| `0x30570`–`0x33db8` | "Condor lib" (hand-written asm, VESA, sprites, fonts, palette, mouse, timer) interleaved with Watcom CRT objects pulled in by the linker (`fopen` `0x309a4`, `fread` `0x30a73`, `fclose` `0x30c30`, `printf` `0x30daf`) |
| `0x33db8`–`0x3409c` | Watcom startup (`_cstart_`), |
| `0x3409c`–`0x350ba` | sprite blitters (unrolled copy code, jump tables built at run time) |
| `0x350ba`–`0x3df0f` | Watcom C runtime + math library (ignore), with a few lib routines in between |

## 1. File / asset layer

**Summary for format work** (all **V**):

- There is **no generic decompressor**. The only compressed formats are `.RON` (GIF/LZW)
  and `.SPT` (row/run encoded, decoded at blit time). `.UNI`, `.3D`, `.MAP`, `.COL`, `.TRJ`,
  `.PCM`, `.STP`, `.TXT`, `DEMOILEn.DAT` are read with plain `fread` into fixed globals.
- `.RON` is a **GIF file XORed with 0x55**; the 6 signature bytes read `RON96a` instead of
  `GIF87a` (only the first 3 are compared, against `"RON"` at `0x42fd4`). Decoder =
  classic GIF87a LZW (variable code width up to 12 bits, sub-blocks ≤ 255 bytes, clear
  code, EOI, interlace supported via tables at `0x41a50`/`0x41a5a`). Every byte read goes
  through `xor 0x55` (`0x2b450`), *except* extension-block skipping (`0x2b680`, raw
  `fgetc`).
- BNK lookup is a linear scan with `strnicmp`/`memicmp`-like compare of 12 bytes
  (`0x35b18`), then `fseek(entry.offset)`; the file handle stays positioned at the entry
  and the caller reads from it. The BNK is reopened for every asset.

| Address | Proposed name | Purpose | Conf |
|---|---|---|---|
| `0x2b4e0` | `bnk_set_archive(const char *path)` | stores archive path pointer in `0x5182c4` | V |
| `0x2b4f0` | `bnk_open_entry(name, mode, u32 *size_out) -> FILE*` | `fopen(archive, mode)`; `fread` u32 count; loop: `fread` 20-byte entry `{char name[12]; u32 offset; u32 size}`, compare 12 bytes with name; on hit `fseek(f, offset, SEEK_SET)`, `*size_out = size`, return FILE*. Returns 0 if not found. FILE* also kept in `0x41a28` | V |
| `0x2bc20` | `load_banque(name, void *dst)` | `bnk_set_archive("BANQUE.BNK")`; `bnk_open_entry(name,"rb",&size)`; `fread(dst,1,size,f)`; `fclose`. Raw copy — used for every `.SPT` (49 call sites) | V |
| `0x2b5f0` | `load_ron(name, u8 *dst, i16 *w_out, i16 *h_out, u8 *pal, int pitch, int decode_pixels)` | `bnk_set_archive("PICTURE.BNK")`, open entry, `ron_decode`, close, then **palette >>= 2** for all 768 bytes (8-bit → 6-bit VGA). Also zeroes the high halves of `*w_out`/`*h_out` (they are 32-bit ints written as i16) | V |
| `0x2b6c0` | `ron_decode(FILE*, pal, dst, i16 pitch, i16 *w, i16 *h, int decode_pixels)` | GIF container parser on XORed stream: 13-byte header (sig + logical screen descriptor), global colour table if `flags&0x80` (`3 << ((flags&7)+1)` bytes → `pal`), skip `'!'` extensions, expect `','`, 9-byte image descriptor (w, h, flags), optional local colour table, LZW min code size byte, then `ron_lzw` | V |
| `0x2b830` | `ron_lzw(FILE*, dst, i16 pitch, i16 min_code_size, w, h, u8 img_flags)` | GIF LZW decoder. Tables: stack `0x5182c8`, suffix `0x5192c8`, prefix (u16) `0x51a2c8`; code masks (u16) at `0x41a30`. Writes each finished line to `dst + pitch*line`; interlaced if `img_flags&0x40` (step table `0x41a50`, start table `0x41a5a`) | V |
| `0x2b480` | `fread_xor55(buf, size, n, FILE*)` | `fread` then XOR every byte with `0x55` | V |
| `0x2b4b0` | `fgetc_xor55(FILE*)` | one byte through `fread_xor55` | V |
| `0x2b450` | `xor55(buf, len)` | in-place XOR `0x55` | V |
| `0x2b680` | `ron_skip_extension(FILE*)` | skips GIF sub-blocks with raw `fgetc` (`0x35d0d`) — not un-XORed, so would misparse; presumably no RON has extensions | V (code), H (never used) |
| `0x12700` | `uni_load(path)` | Loads `ILE_n.UNI` with plain `fread`, in file order: palette `0x300` → `0x4f0b00`; `0x2000` bytes → `0x485750`; `u32 nblocks` → `0x4f0ec4`; `u32 noffsets` (local); `nblocks*256` bytes → `0x2c4498`; `noffsets*4` bytes (u32 offsets into the block data, converted to pointers in `0x487750`) ; `0x20000` bytes → `0x2a4498`; `0x10000` bytes → `0x22cdc8`; `0x10000` bytes → `0x261b88` (height map: used to build a 256×256 i16 table at `0x1f2668`, then zeroed). Also builds a 512-entry cosine table at `0x1cb3a8`. Path is `C:\PUNKY\ILE_n.UNI` with the drive letter patched from `0x1c6ee4` (CD/install drive) | V (read order), H (meaning of blocks) |
| `0x13390` | `uni_init(path)` | sets `0x4f0ec0 = [0x126e8]*[0x126e4]`, reciprocal in `0x4f0e44`, calls `uni_load` | V |
| `0x141a0` | `objets3d_load(name_3d, name_map)` | `.3D`: `u32 nobjects` (`0x4005c`), `u32 npoints` (`0x4f0e50`), `u32 nfaces` (`0x4f0e4c`), 1 unused u32, then `nobjects` × 4 × u32 `{first_point?, first_face?, count, count}` stored as ranges `[a, a+c-1]` in `0x271b88` (16 bytes each), then `npoints*12` bytes (3 × f32, scaled by the f64 at `0x42457`) → `0x23d368`, `nfaces*24` bytes → `0x1cc508`. Limits: `npoints*3 ≤ 0x20d0`, `nfaces*3 ≤ 0x1964`. `.MAP`: `u32 ntex` (`0x4f0e54`), `u32 n` (`0x4f0e58`), `0x300` bytes palette (discarded — overwritten), `n*12` bytes → `0x1cbba8`, `ntex*256` bytes of 16×16 texture blocks → `0x48aac0` | V (read order), H (field meaning) |
| `0x24000` | `trj_load_monsters()` | opens `MonsterN.Trj` (N = island digit from `0x517f88`), reads up to `0x2800` bytes → `0x4f6144`; header: `u32 n` then `n` × 8-byte records, then `u32 m` (must be ≤ `0xd1`), `m` × 32-byte records, then another counted table. Pointers kept in `0x414a8`–`0x414bc` | V |
| `0x15ce0` `0x17680` `0x19030` `0x1aa80` `0x1c760` | `ileNN_trj_load` (islands 5,1,2,3,4) | per-island loaders for `ileNN.trj` | H (string refs only) |
| `0x25140` | `txt_get_dialogue(int id) -> char*` | opens `ILE_n.TXT` (name at `0x416e8`, digit patched), `fgets` lines until one *starts with* `sprintf("%d", id)`, skips `language+1` further lines (`0x416e4` = language index), then extracts 4 quoted strings into `0x50d1c4`, `0x50d2c4`, `0x50d3c4`, `0x50d4c4` (256 bytes each) | V |
| `0x25030` | `demo_open()` | opens `DEMOILEn.DAT` for read, or write if `0x41600` (record flag) | V |
| `0x25070` | `demo_step()` | record: writes the 16-byte input state `0x50d5c4` per tick; playback: reads 16 bytes per tick, sets `0x50d5d0` at EOF | V |
| `0x250f0` | `demo_close()` | | V |
| `0x258c0` / `0x25a40` | `settings_load` / `settings_save` | `GAME.STP`, `SAVEGAME.REC` (slot names `FREE 1`..`FREE 8`) | H |
| `0x26050` / `0x26100` | `savegame_write` / `savegame_read` | `SAVEGAME.__0` (digit patched per slot) | H |
| `0x2ef80` | `sample_load(name, u32 *size_out) -> void*` | `bnk_set_archive("c:\PUNKY\SAMPLE.BNK")` (drive letter patched), open entry, `malloc(size)`, DPMI lock (`0x2f100`, int 31h AX=0600h), `fread` whole entry. Raw PCM, no header | V |
| `0x2f020` | `samples_load_all()` | loads the 29 names from the pointer table at `0x41a84` into `0x51caf4[]` (ptr) / `0x51cb68[]` (size) and fills 42-byte voice descriptors at `0x51cbdc` | V |
| `0x2c030` | `flc_open(player*, path, bool to_buffer)` | reads 128-byte FLC header, accepts magic `0xAF11`/`0xAF12` | V |
| `0x2c1e0` | `flc_next_frame(player*)` | decodes one frame (chunk handlers via jump table at `0x2c1a4`) | H |
| `0x2c160` | `flc_close(player*)` | | V |
| `0x2bc70` | `flc_play(path, loops)` | open, per frame: wait N retraces, decode, set palette if changed, blit 640×480; close; clear screen | V |

### SPT layout as used by the blitter (`0x340e8`) — **V**

`base` = start of the loaded file (pointer set per bank in `0x51f484`).

- `u32 count` at `base`.
- Directory entry `i` at `base + 4 + 16*i`: `u32 data_off`, `u32 size` (not read by the
  blitter), `i16 w` (+8), `i16 h` (+10), `i16 off_x` (+12), `i16 off_y` (+14).
  Draw position = `(x + off_x, y + off_y)`; the blitter publishes `-off_x`, `-off_y`,
  `w`, `h` in `0x51f488`, `0x51f47c`, `0x51f480`, `0x51f48c`.
- Sprite data at `base + 4 + data_off`: 8-byte header, `u16 rows` at +2; rows follow at +8.
- Row: `u16 nruns`, `u16 row_size` (bytes from the start of the row to the next row, used
  to skip clipped rows), then `nruns` × run.
- Run: `u8 x` (offset from the row's left edge), `u8 x_mirrored` (start offset used by
  the mirrored blitter `0x34468`, which writes right-to-left), `u16 len`, then `len` raw
  pixels copied opaquely. Transparency = gaps between runs.
- Clipping is vertical only, by comparing the linear offset `y*pitch + x` with
  `clip_top*pitch` (`0x41dfc`) and `clip_bottom*pitch` (`0x41e00`). No horizontal clip.
- The row pointer is advanced by one pitch *before* the row is written (`0x3416b`), so
  row 0 lands on line `y + off_y + 1`. Same in the mirrored and scaled variants.


Other loaders seen while mapping (**V**): `0x33440` `col_load(path, u8 *pal)` reads 8 bytes
(skipped) then `0x300` palette bytes and shifts each `>> 2` — `.COL` is an 8-byte header +
8-bit palette. `GAME.STP` (`0x258c0`) is 8 × u32 in this order: sound on (`0x41610`), CD
music on (`0x41614`), controller 0=keyboard/1=joystick (`0x41618`), sample volume 0–15
(`0x4161c`), CD volume (`0x41620`), `0x41624` (unknown), language index (`0x416e4`),
speed adjust (`0x4000c`). `SAVEGAME.REC` is 8 × 10-byte slot names.

## 2. Startup, main loop, state machine, timing

Details: `engine/states.md`, `engine/timing.md`.

| Address | Proposed name | Purpose | Conf |
|---|---|---|---|
| `0x33db8` | `_cstart_` | Watcom startup (jumps over the copyright string) | V |
| `0x12050` | `main` | whole top-level state machine, never returns normally (exit via `exit()` from the menu) | V |
| `0x11ef0` | `sys_init` | CD drive detection (`0x1e4d0`, looks for a marker file on drives B..Z, drive index in `0x1c6ee4`), MSCDEX init, sound driver init + sample load | V |
| `0x1e4d0` | `cd_find_drive(name)` | loops drives 1..25, MSCDEX drive check, tries `fopen("X:\\...")` | V |
| `0x11fc0` | `island_enter(n)` | BIOS mode 13h, back buffer pointer = `0x4a5ae0`, calls per-island asset loader | V |
| `0x25b00` | `menu_run` | loads settings, starts CD track 4, loads `MENUSPIC.RON` + fonts, runs main menu, saves settings. Results in `0x50d5e0` (continue/loaded game), `0x50d5e1` (quit), `0x50d5e2` (new game) | V |
| `0x25ed0` | `menu_main` | `MAIN MENU` loop; entries → `0x27880` new game/cheat, `0x264f0` load, `0x265c0` save, `0x26660` options, `0x26e30` exit | V |
| `0x25470` | `menu_draw(title, items, selected)` | draws a menu page | H |
| `0x25c90` | `menu_input(int *sel, int count)` | menu navigation; last key in `0x51d2f0` | H |
| `0x26660` `0x26840` `0x26980` `0x26bc0` `0x26c70` `0x26d10` | `menu_options`, `_resolution`, `_sound`, `_controller`, `_calibrate`, `_joytest` | option pages (names from title strings) | H |
| `0x26280` | `menu_enter_name` | save-slot name entry | H |
| `0x27880` | `menu_new_game` | starts a game; with cheat flag offers `CHEAT MODE` island select (sets `0x517f88` = 1..5) | H |
| `0x292d0` | `game_reset(int keep)` | resets player state for a new game / island (lives `0x517f0c = 3`, …) | H |
| `0x13390`/`0x12700` | `uni_init`/`uni_load` | see section 1 | V |
| `0x415b8[n]` → `0x215b0` `0x21640` `0x216d0` `0x21760` `0x217f0` | `island_n_reset` | per-island state init called through the table at `0x415b8` | H |
| `0x2a260` | `game_tick_frame` | one rendered frame of island logic: timers, `logic_substeps`, then per-frame gameplay (`0x11400`, `0x10290`, `0x11be0`), CD loop check (`0x11f70`), view-mode keys (`0x11960`) | V |
| `0x28d50` | `logic_substeps` | input poll + fixed-rate catch-up loop (see timing.md) | V |
| `0x25070` | `demo_step` | per substep: record/replay 16-byte input record | V |
| `0x13480` `0x14040` `0x10860` `0x103b0` | substep functions | per-tick simulation (player `0x10860`, world `0x103b0`) — **gameplay**, see `game/` | H |
| `0x11960` | `view_keys` | F1 cycles view mode `0x4002c` (0/1/2), F2 toggles interlace `0x40044`, Backspace cycles weapon, Enter toggles a timed item | V |
| `0x11f70` | `cd_loop_check` | every 10th frame: if CD not busy, replay track 5 | V |
| `0x28870` | `show_picture(ron_name, fade_steps)` | fade out, mode 0x101, load RON, blit, fade in, wait key press+release, fade out | V |
| `0x2bc70` | `flc_play(path, loops)` | section 1 | V |
| `0x255f0` | `game_over_screen` | `n.RON`, `CONTINUE 9` countdown; sets `0x50d5e2` if continuing | H |
| `0x280f0` | `map_screen(ronA, ronB)` | island map (`CarteNN.RON`), entered from the per-island frame function | H |
| `0x28e00` | `dialogue(id)` | dialogue box (`Dial_n.RON`), 170 call sites | H |
| `0x2a450` | `shop_frame` | merchant (`MARCHAND.RON`, `Hand.spt`), uses mouse hot zones `0x2b370`/`0x2b3a0`/`0x2b400` | H |
| `0x246b0` | `hud_draw` | HUD sprites (30 calls to the sprite blitter) | H |
| `0x2ff60` / `0x2fdfd` | `crash_handler` / `crash_map_lookup` | exception screen, looks the address up in `P.MAP` | V |

### Per-island modules (same shape for each island) — names **H**, call structure **V**

| Role | Isl. 1 | Isl. 2 | Isl. 3 | Isl. 4 | Isl. 5 |
|---|---|---|---|---|---|
| asset load (`ILE_n.UNI`, `Objets_n.3d/.Map`, SPT banks) | `0x174c0` | `0x18e70` | `0x1a8c0` | `0x1c5a0` | `0x15b10` |
| trajectory load/init (`ile0n.trj`) | `0x17680` | `0x19030` | `0x1aa80` | `0x1c760` | `0x15ce0` |
| scripted events / vehicle section (`radeau`, `ulm`, `Jetski`/`Radar`, `Biplan`/`Tour`, `Commando`/`Bombe`) — sets next island in `0x517f04` | `0x178d0` | `0x19300` | `0x1ae00` | `0x1cae0` | `0x160d0` |
| inventory screen (`Inv_0n.RON`) | `0x185a0` | `0x19fd0` | `0x1bc30` | `0x1d980` | `0x16d60` |
| section that switches to CD track 6 | `0x18160` | `0x1a200` | `0x1be60` | `0x1dbb0` | `0x16f90` |
| (unknown) | `0x184a0` | `0x1a5d0` | `0x1c350` | `0x1df80` | `0x172d0` |
| per-frame driver: calls the above, `map_screen`, places static 3D objects (`0x14050`), then `render_frame` (`0x148a0`) | `0x187e0` | `0x1a6d0` | `0x1c450` | `0x1e080` | `0x173d0` |
| first call of the frame driver | `0x188e0` | `0x1a810` | `0x1c4f0` | `0x1e0e0` | `0x17410` |

**Gameplay logic starts here (see `game/`):** `0x100be`–`0x11e90` (player/world simulation),
the per-island event functions above, `0x21588`–`0x25000` (monsters, bosses, trajectories,
not fully function-ised by Ghidra: tables at `0x21588`, code from `0x218b0`),
`0x27de0`–`0x2a450` (bonus/inventory helpers `0x27de0`, `0x27e50`, `0x27ec0`, `0x27f30`,
`0x27fd0`; dialogue; shop).

## 3. Renderer

Details: `engine/renderer.md`.

| Address | Proposed name | Purpose | Conf |
|---|---|---|---|
| `0x31c60` | `vesa_init(mode, pitch)` | VBE 4F01h mode info via real-mode int (DOS buffer), window segment(s) → `0x41db4`/`0x41db8`, granularity shift `0x51e450`, window mask `0x51e448`, row offset table `0x51d448`, bank table `0x51d408` | V |
| `0x323d0` | `vesa_set_mode(mode)` | int 10h AX=4F02h. Always called with `0x101` (640×480×256, banked, no LFB) | V |
| `0x323ed` | `vesa_set_bank(n)` | int 10h AX=4F05h | V |
| `0x30dd4` | `bios_set_mode(al)` | int 10h AH=0 (used with `0x13` and `3`) | V |
| `0x30e50` | `gfx_init_rows(pitch)` | builds row offset table `0x530574[1024]`, sets draw target `0x531574 = 0xA0000`, builds two jump tables (`0x531578`, `0x531708`) into unrolled code | V |
| `0x531574` | `g_draw_target` | pointer all primitives draw to; normally the back buffer `0x4a5ae0` (640×400+, pitch 640) | V |
| `0x32b80` | `set_clip(x0,y0,x1,y1)` | → `0x41dcc`,`0x41dd0`,`0x41dd4`,`0x41dd8` (inclusive) | V |
| `0x33600` | `sprite_set_target(dst, dst2, pitch, pitch2, x0, y0, x1, y1)` | sets sprite pitch `0x41dec` and vertical clip products `0x41dfc`/`0x41e00`, rebuilds sprite jump tables (`0x3409c`) | V |
| `0x32708` | `vesa_blit(src, sx, sy, x1, y1, dx, dy, src_pitch)` | copy a rectangle of a memory buffer to the banked VESA screen (handles bank crossings) — the "present" call | V (args), H (internals) |
| `0x305c4` | `blit(src, sx, sy, w-1?, h, dst, dx, dy, src_pitch, dst_pitch)` | memory-to-memory rectangle copy (self-modifying pitches) | V |
| `0x32a04` | `vesa_fill_rect(x0,y0,x1,y1,color)` | fill on the VESA screen | V |
| `0x310bb` / `0x30f3b` / `0x30fc9` / `0x3102d` | `fill_rect` / `hline(x0,y,x1,c)` / `vline` / `rect_outline` | on `g_draw_target` | V |
| `0x3240a` | `vesa_putpixel(x,y,c)` | clipped, banked | V |
| `0x30ddf` / `0x30df2` | `wait_vretrace` / `wait_hretrace(n)` | port `0x3DA` bit 3 / bit 0 | V |
| `0x35590` | `dac_set(index, r, g, b)` | ports `0x3C8`/`0x3C9`, 6-bit | V |
| `0x331a0` | `palette_set(first, last, u8 *pal)` | loop of `dac_set` | V |
| `0x331f0` | `palette_fade(first, last, steps, u8 *from, u8 *to)` | `steps+1` iterations, one `wait_vretrace` each, linear interpolation `from + (to-from)*i/steps` | V |
| `0x340e8` | `sprite_draw(index, x, y)` | SPT blitter (section 1), bank base in `0x51f484` | V |
| `0x34468` | `sprite_draw_mirrored(index, x, y)` | right-to-left (uses `bswap`) | V |
| `0x34558` / `0x350ce` | `sprite_draw_scaled` / `_scaled_mirrored(index, x, y, sx, sy)` | 8.8 shrink-only scaling; **unreferenced** (only called from `0x33800`) | V |
| `0x33800` | `sprite_list_draw(first, last)` | draws 68-byte sprite-list entries at `0x51e470`; **no caller, no pointer** — dead library code | V |
| `0x2d940` | `sprite_unpack(index, …)` | decodes an SPT sprite into the 256-wide scratch buffer `0x261b88` (for world sprites) | V |
| `0x32bb0` | `font_set(spt_base, first_char, spacing)` | font = SPT bank, glyph index = `char - first_char` | V |
| `0x32ce0` / `0x32c10` | `text_draw(x, y, str, align)` / `text_measure` | align 0 left, 1 centre, 2 right; advance = glyph w + spacing | V |
| `0x148a0` | `render_frame` | camera, sort, 3D objects, ground, sky, HUD, present | V |
| `0x2b1c0` | `camera_update` | derives eye/target from player position/heading | V |
| `0x152d0` | `camera_build_matrix` | look-at basis + screen scale | V |
| `0x100be` (`0x15590` wrapper) | `project_points(f32 xyz[], i32 sx[], i32 sy[], n, i16 depth[])` | asm, perspective projection | V |
| `0x128c0` | `ground_build_vertices` | 30×30 height-field vertices around the camera, project, depth-sort cells (`0x158c0` quicksort) | V |
| `0x14b60` | `ground_draw` | back-to-front cells; interleaves 3D-object faces and world sprites by depth | V |
| `0x12bd0` | `ground_draw_triangle(tile, half, i0, i1, i2)` | cull, set UVs for the 4 tile rotations, textured fill + fog pass | V |
| `0x2da30` (+ span `0x2e0e4`) | `tri_textured_opaque` | affine textured triangle, 8.8 UV, 256-wide texture page | V |
| `0x2c838` (+ span `0x2ceec`) | `tri_textured_keyed` | same, colour 0 transparent (3D objects, scaled sprites) | V |
| `0x2d158` (+ span `0x2d717`) | `tri_fog` | Gouraud-interpolated level, remaps existing pixels through the fog table `0x485750[level*256 + pixel]` | V |
| `0x13c40` | `objects3d_transform` | transforms/projects instances placed with `0x14050`, builds face list | H |
| `0x14050` | `object3d_place(slot, model, x, y, z, rx, ry, rz, flag)` | 72-byte instances at `0x23cdc8` | V |
| `0x138f0` | `world_sprite_add(…)` | 44-byte billboard entries at `0x489990`, max 99, culled beyond 4500 units | V |
| `0x13300` / `0x130f0` | `world_sprites_sort` / `_project` | | V |
| `0x13500` | `world_sprite_draw` | unpack, fog-shade (`0x306dc`) or silhouette (`0x30745`), rotate/scale corners, two `tri_textured_keyed` | V |
| `0x144a0` | `sky_draw` | copies rows of `Ciel_n.RON` (`0x1980d0`), horizontal scroll from heading, horizon at `0x4f0e64` | V |
| `0x13010` | `ground_height_at(x, z)` | bilinear interpolation in the i16 height table `0x1f2668` | V |
| `0x146f0` | `view_mode_apply` | switches video mode / clip for view mode 0/1/2 | V |
| `0x11b40` / `0x11ae0` | `animaps_tick` / `animap_copy` | animated ground tiles: copies 32×32 frames from `Animaps.RON` (`0x21c5c8`, 192 wide) into tile textures | V |
| `0x1f410` `0x1fb70` `0x202d0` `0x20a30` `0x20de0` (+ `0x1ea60`–`0x1f220`) | C triangle fillers | **unreferenced** (no call, no pointer fixup) — dead code superseded by the asm fillers | V |

## 4. Sound

Details: `engine/sound.md`.

| Address | Proposed name | Purpose | Conf |
|---|---|---|---|
| `0x2f1bc` | `sound_init` | allocs DOS memory, loads `SOUNDRV.COM` (`0x2ebb6`), inits it (`0x2ec35`), picks mode from driver caps, DMA setup, installs 70 Hz timer (`0x2fa30(0x4295, 0x2f832)`) | V |
| `0x2f559` | `sound_set_mode(caps)` | 16-bit stereo / 16-bit mono / 8-bit mono; 16000 Hz; buffer 630 samples | V |
| `0x2e350` | `mixer_init(mode)` | clears 8 channels, selects mix routine into `0x51c4f0` | V |
| `0x2e4d0` `0x2e5d0` `0x2e6d0` `0x2e7d0` | `mix_8m`, `mix_8s`, `mix_16m`, `mix_16s` | fill one 630-sample buffer | V (8m, 16m) |
| `0x2f832` | `timer_tick` | 70 Hz: `g_ticks++` (`0x41b02`); if the driver wants data, mix next half-buffer and hand it over | V |
| `0x2ef10` | `sfx_play(sample, volume)` | picks a channel (`0x2eeb0`: free one in 1..7, else the one closest to its end) and starts the sample | V |
| `0x2ef40` | `sfx_play_loop(sample, volume)` | channel 0, infinite loop (`loop count = -1`) | V |
| `0x2e430` | `channel_start(ch, desc*, vol)` | | V |
| `0x2ef70` | `channel0_set_volume(v)` | | V |
| `0x2f020` / `0x2ef80` / `0x2f0d0` | `samples_load_all` / `sample_load` / `samples_free` | section 1 | V |
| `0x2f4d4` | `sound_shutdown` | | V |
| `0x18a78`–`0x18dec` | `drv_*` | thunks into the real-mode driver through int 66h (`VECTOR.COM` installs the vector) | H |
| `0x1e420` | `cd_init(drive)` | MSCDEX presence, DOS buffer for request headers | V |
| `0x1e740` | `cd_play_track(n)` | reads track start (IOCTL 0Bh) of n and n+1, PLAY AUDIO (cmd 84h) for that length | V |
| `0x1e860` / `0x1e780` | `cd_stop` | STOP AUDIO (cmd 85h), issued twice | V |
| `0x1e880` | `cd_is_busy` | IOCTL input, returns `0x200` when playing | V |
| `0x1e950` | `cd_set_volume(v)` | IOCTL output 03h, all 4 channels | V |

## 5. Input

Details: `engine/input.md`.

| Address | Proposed name | Purpose | Conf |
|---|---|---|---|
| `0x31b90` / `0x31be0` | `kbd_install` / `kbd_remove` | hooks int 9 with `0x358f8`, clears key table | V |
| `0x358f8` | `kbd_isr` | port 60h; `key[scancode] = 0xFF` on make, `0` on break; `E0` prefix → index + `0x80`; `E1` (Pause) toggles `0x51d304` | V |
| `0x51d1f0` | `g_keys[256]` | key table; last make code in `0x41c71` | V |
| `0x26ec0` | `input_poll(int gameplay)` | builds the 16-byte input record `0x50d5c4` from keys/joystick, edge detection, cheat codes, pause, F12 screenshot (`SCRSHTnn.RAW`), idle → demo timer | V |
| `0x2a2f0` | `input_any()` | poll + "any button" test | V |
| `0x27a30` / `0x27ab0` / `0x27b10` / `0x27c70` | `joy_buttons` / `joy_detect(axis)` / `joy_read_axes` / `joy_poll` | game port `0x201` | V (first two), H |
| `0x3350e` | `mouse_read` | int 33h AX=3 → x `0x5337dc`, y `0x5337e0`, buttons `0x5337e4` | V |
| `0x31dd0`–`0x32300` | mouse cursor draw/restore helpers | used by the shop | H |
| `0x35982` / `0x359a3` / `0x359be` | `bios_getkey` / `bios_kbhit` / `kbd_flush` | int 16h | H |

## Watcom C runtime (ignore)

Identified by behaviour (**V** unless noted): `0x309a4` fopen, `0x30a73` fread, `0x30c30`
fclose, `0x355da` fwrite, `0x35bba` fseek, `0x35ae3` rewind (H), `0x35d0d` fgetc, `0x357b9`
fgets, `0x30daf` printf, `0x35543` sprintf, `0x35835` strstr, `0x35480` strcmp (H),
`0x35b18` memicmp/strnicmp, `0x33110` memset, `0x35e82` malloc, `0x35f85` free, `0x33132`
kbhit, `0x33149` getch, `0x33165` exit, `0x35576` abort-style exit (H), `0x30570` rand (H),
`0x305a4` `__CHP` (float→int helper before `fistp`; truncates toward zero, verified by the renderer port), `0x36016` `_dos_getvect`,
`0x3604b` `_dos_setvect`, `0x360b9` int386, `0x360e7` int386x, `0x34084` load DS for
ISRs, `0x3538c` set critical-error handler (H). Not CRT but generic: `0x352a0`
`dpmi_realmode_int(intno)` (DPMI 0300h with the register block at `0x51f4c8`), `0x35240`
`dos_alloc(paragraphs)` (DPMI 0100h). Everything from `0x35a00` to `0x3df0f` not listed
above is runtime/math library.
