# `ILE_n.UNI` — island terrain

Parser: `crates/pd-formats/src/uni.rs`. Dump: `pd-extract world` (`ile_n.png`, `_tiles`,
`_height`, `_attr`, `_shade`, `_uni.json`).

The file is **not a picture and not compressed**. The island is a 256x256 cell height-mapped
terrain; each cell is two textured triangles that show one 32x32 tile. (The earlier
"`0x3E0000` = 2048x1984" reading was wrong: those bytes are palette entries 0 and 1.)
The game opens it as `C:\PUNKY\ILE_n.UNI` with the drive letter and digit patched in
(`0x1213c`, `0x174c9`).

## Layout — **verified** (loader `0x12700`, all sizes are the `fread` lengths)

| Offset | Size | Loaded to | Content |
|---|---|---|---|
| `0x0000` | `0x300` | `0x4f0b00` | palette, 256 x RGB, 6-bit VGA values |
| `0x0300` | `0x2000` | `0x485750` | 32 shade tables x 256 bytes |
| `0x2300` | 4 | `0x4f0ec4` | `sheet_rows` (u32) |
| `0x2304` | 4 | local | `tile_count` (u32) |
| `0x2308` | `sheet_rows * 256` | `0x2c4498` | tile sheet, 256 pixels wide, 8-bit indexed |
| … | `tile_count * 4` | `0x487750` | u32 byte offset of each tile's top-left texel in the sheet |
| … | `0x20000` | `0x2a4498` | cell map, 256x256 x u16 |
| … | `0x10000` | `0x22cdc8` | attribute map, 256x256 x u8 |
| … | `0x10000` | `0x261b88` | height map, 256x256 x u8 |
| … | `tile_count * 4` | — | never read; the file is closed after the height map |

On disc the sizes add up exactly for all five islands (unit test `disc_islands`). **verified**

| Island | sheet_rows | tile_count |
|---|---|---|
| 1 | 6080 | 1519 |
| 2 | 5536 | 1378 |
| 3 | 5376 | 1341 |
| 4 | 5440 | 1356 |
| 5 | 4320 | 1076 |

## Indexing and world coordinates — **verified**

- All three maps are indexed `x * 256 + z` (cell map as `(x << 9) + z * 2`): `0x129df`,
  `0x12ab3`, height lookup `0x13071`.
- One cell is 300 world units (`0x126e4` 20 x `0x126e8` 15, product kept at `0x4f0ec0`).
  Cell of a world coordinate: `(w + 128 * 300) / 300` (`0x13025`). World origin is the centre of
  the map (cell 128,128); the map spans -38400..+38400.
- The game's own map screen draws first index along screen X and second index along screen Y
  (`0x2825e`), so `pd-extract` writes pictures with column = x, row = z.

## Cell map (u16) — **verified** (`0x12d7f`..`0x12f3f`)

- bits 0-11: tile number (index into the offset table).
- bits 13-14: texture rotation `r`.
- bits 12 and 15: never set on disc; not tested by the code that was read.

A cell (x,z) is drawn as triangle 0 = corners (x,z), (x+1,z), (x,z+1) and triangle 1 =
(x+1,z+1), (x+1,z), (x,z+1) (`0x14c39`). Texel for in-cell position (dx,dz), each 0..31,
u = column, v = row in the sheet (`tile_offset + v*256 + u`):

| r | u | v |
|---|---|---|
| 0 | dx | dz |
| 1 | dz | 31-dx |
| 2 | 31-dx | 31-dz |
| 3 | 31-dz | dx |

Which of the two coordinate sets (`0x4f0af0..8`, `0x4f0ae0..8`) is the column was settled by
rendering: with u = `0x4f0af0` set the tiles join seamlessly, swapped they do not. **verified**
by output (island pictures are coherent for all five islands and match a DOSBox capture of
island 1: grass, brown rocky patches, ponds).

Tile offsets on disc are multiples of 32 and tiles are laid 8 per 256-pixel row band.

## Height map — **verified**

Loader converts each byte to `i16 = byte * 3.0` (`0x423ef`) into `0x1f2668` (`0x1283a`), then
clears the byte buffer. World height = byte x 3. Height at a world position is bilinear
between cell corners (`0x13010`). Max byte on disc: 86 / 185 / 141 / 144 / 78 (islands 1-5).

## Attribute map

- 0: free land. **verified**: the random bonus placer (`0x28bc0`) accepts a cell only when the
  3x3 block around it is all 0.
- 1 and 13: surface is animated — height gets `cos` wave from a 512-entry table of amplitude 5
  (`0x129e7`, table built at `0x127ea`). 1 is the sea / ponds. **verified** (code);
  "13 = second animated liquid" is **hypothesis**.
- 6: animated with double amplitude (`0x12a56`). Only island 5 (lava) uses it. **verified** (code).
- Other values on disc (2, 3, 5, 7-10, 12, 15-24): read by per-island gameplay code
  (`0x17431`, `0x188fe`, `0x1a831`, `0x1c511`, `0x1e101`); meanings not established.
  In the dumps value 2 outlines coasts on islands 1-2 and several values mark single
  rectangles (buildings/zones). **hypothesis**: collision and trigger zones.

## Shade tables — partly verified

32 tables of 256 bytes, used as `table[level * 256 + colour]` (`0x1357f`, rasteriser
`0x2d861`..). Level is computed from depth as `(depth - (far - range)) * 64 / range`
(`0x14fcf`). **verified** (code). Dumped through the palette, level 0 maps every colour to the
horizon/sky colour and higher levels approach the plain colour, i.e. distance fog.
**verified** by output; exact level-to-distance mapping not worked out.

## Palette

6-bit VGA. Equals `ILE_n.COL >> 2` except for a few bytes. The game overwrites it with the
sky picture's palette and then `ILE_n.COL` (same buffer `0x4f0b00`, `0x1221a`..`0x12249`), so
the COL palette is the one on screen. **verified**

## Trailer

`tile_count` x 4 bytes of palette indices, unread by `P.EXE`. **hypothesis**: per-tile
low-detail colours left by the editor.
