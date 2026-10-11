# `.RON` pictures

Code: `crates/pd-formats/src/ron.rs`. Addresses are `P.EXE` linear addresses.

**A `.RON` file is a GIF file with every byte XORed with `0x55` and the signature `GIF`
replaced by `RON`.** — **verified**: loader `0x2b5f0` → container parser `0x2b6c0` → LZW
decoder `0x2b830`; XOR helper `0x2b450` (`xor bl, 0x55`), applied to everything read through
`0x2b480` / `0x2b4b0`. All 55 pictures decoded by `ron.rs` are pixel-identical to the output
of an independent GIF decoder (Pillow) fed the un-XORed bytes with the signature patched.

## Layout after XOR — **verified**

| Offset | Size | Content |
|---|---|---|
| 0 | 3 | `"RON"` — the only bytes the game checks (`0x2b6e6`, 3-byte compare with `0x42fd4`) |
| 3 | 3 | `"96a"` (ignored) |
| 6 | 2+2 | logical screen width, height |
| 10 | 1 | flags; `0x80` = global colour table follows, `(flags & 7) + 1` = bits per index. `0xd7` in every file |
| 11 | 2 | background index, aspect (both 0) |
| 13 | 768 | global colour table, 256 × RGB, **8 bits per component** |
| | 1 | `0x2c` image separator (`0x21` extension and `0x00` are tolerated before it; no file has any) |
| | 9 | left, top, width, height (u16 each), flags (`0x40` = interlaced, `0x80` = local colour table) |
| | 1 | LZW minimum code size; the game rejects anything outside 2..=8 (`0x2b860`) |
| | … | LZW data in sub-blocks (length byte + up to 255 bytes), terminated by a zero length |

Every shipped file: no extensions, descriptor flags 0 (not interlaced, no local table), code
size 8, 256-colour global table.

## Pixel codec — **verified** (`0x2b830`)

Standard GIF LZW: codes packed LSB first, width starts at `code_size + 1` and grows to 12
bits, clear code `1 << code_size`, end code `clear + 1`, first free code `clear + 2`, table
frozen at 4096 entries. The decoder fills a line buffer and copies each completed line to
`dst + pitch * row`; with the interlace flag it steps rows by 8, 8, 4, 2 starting at 0, 4, 2, 1
(tables `0x41a50`, `0x41a5a`). On a code greater than the next free code, or on running out
of data, it stops and keeps what it has (`Ron::complete == false` in our decoder).

## Palette — **verified**

- The file stores 8-bit components. After decoding, `0x2b658` shifts all 768 palette bytes
  right by 2, giving the 6-bit values programmed into the VGA DAC. The low two bits of the
  file values never reach the screen.
- `Ron::palette` keeps the file bytes, `palette_vga6()` gives what the game uses, and
  `palette_rgb8()` expands 6 → 8 bits by bit replication, `(v << 2) | (v >> 4)` (0 → 0,
  63 → 255). **Verified** against a DOSBox-X capture of the main menu: every pixel outside
  the dimmed menu box equals the decoded `MENUSPIC.RON` with this expansion (plain `v << 2`
  matches only 9 % of the frame).

## Loader call — **verified** (`0x2b5f0`)

`load_ron(name, dst, &w, &h, palette, pitch, decode_pixels)`: always reads from
`PICTURE.BNK`. `pitch` is the destination row stride (640 for full screens, `0xc0` for
`ANIMAPS.RON`).

## Sizes — **verified** (unit test `disc_pictures`)

| Pictures | Size |
|---|---|
| `ANIMAPS.RON` | 192 × 352 (strip of animated map tiles) |
| `CIEL_1..5.RON` | 640 × 300 (sky behind the island) |
| all others (48 in `PICTURE.BNK`, plus `INSTALL.RON`) | 640 × 480 |

`CIEL_n.RON`'s palette is loaded into the game palette buffer `0x4f0b00` and then
overwritten by `ILE_n.COL` (`0x12237`, `0x12249`).
