# `.SPT` sprite sheets and fonts

Code: `crates/pd-formats/src/spt.rs`. Addresses are `P.EXE` linear addresses.

The game reads a whole sheet into memory (`0x2bc20`, always from `BANQUE.BNK`) and the
blitters walk it in place. There is no decompression step.

## File layout — **verified** (blitter `0x340e8`)

```
u32 count
count × entry (16 bytes)
sprite data, back to back in table order
u8  palette[768]          // see "Trailing palette"
```

Entry:

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | `offset` of the sprite data, **relative to file offset 4** (`data = sheet + 4 + offset`) |
| 4 | u32 | `size` of the sprite data in bytes |
| 8 | i16 | `width` |
| 10 | i16 | `height` |
| 12 | i16 | `hot_x` |
| 14 | i16 | `hot_y` |

The blitter draws the top-left corner at `(x + hot_x, y + hot_y)` (`0x34103`–`0x34144`:
`y - (-hot_y)`, `x - (-hot_x)`). Hotspots are usually negative, e.g. `(-w/2, -h + 1)`: the
anchor is the bottom centre.

Sprite data:

```
i16 width, height, hot_x, hot_y        // copy of the table fields; the blitter reads only
                                       // height (offset 2) as the row count
height × row
```

Row:

```
u16 run_count
u16 row_bytes                          // size of this row including these 4 bytes;
                                       // used to skip clipped rows
run_count × {
    u8  x                              // column of the first pixel, from the left edge
    u8  mirror_x                       // column of the first pixel when mirrored
    u16 n                              // pixel count
    u8  pixels[n]                      // palette indices, copied verbatim
}
```

- An empty row is `00 00 04 00`.
- Transparency is positional: everything not covered by a run is left untouched. There is
  **no colour key**; index 0 inside a run is drawn as colour 0. `Sprite::mask` records
  coverage.
- `mirror_x == width - 1 - x` in all 2541 sprites (**verified**, unit test). The mirrored
  blitter `0x34468` starts at `mirror_x` and writes the run right to left (`bswap` per dword).
- `x` and `mirror_x` are bytes, so a run cannot start beyond column 255.
- Clipping in `0x340e8` is per row only (top/bottom limits `0x41dfc` / `0x41e00`, already
  multiplied by the pitch); there is no horizontal clipping.
- Other blitters over the same data: `0x34558` scaled (scale ≤ `0x100`), `0x350ce` scaled
  and mirrored (reads `mirror_x`). Not analysed further here — **hypothesis** on their exact
  parameters; see `functions.md`.

## Trailing palette — layout **verified**, purpose **hypothesis**

Every shipped sheet ends with 768 bytes after the last sprite: 256 RGB triples with
**6-bit** components (max 63). `SpriteSheet::trailing_palette()` returns it.

- `0x2bc20` copies it into memory with the rest of the file but returns no size, and no
  blitter reads past the sprite data: the game does not use it (**hypothesis**: authoring
  tool left-over).
- It is not reliable. For the indices a sheet actually uses it often equals an island
  palette, but not always the right one: `ENEMY4.SPT` and `BOSS1.SPT` carry island 3's
  colours, `BONUS_5.SPT` and `ENEMY5.SPT` island 4's, `BONUS_2.SPT` matches island 1 for only
  94 of its 132 used colours (and island 2 no better).

## Which palette applies — **verified**

Sprites are drawn with whatever palette is active:

| Sheets | Palette | Evidence |
|---|---|---|
| `BONUS_n`, `ENEMYn`, `BOSSn`, `ICONE_n` | `ILE_n.COL` | island setup `0x12212`–`0x12249` loads `CIEL_n.RON`'s palette then overwrites it with `ILE_n.COL` (`0x33440`: 8-byte header, 768 bytes of 8-bit RGB, each `>> 2`) |
| `DUCK`, `DUCK_1`, `NUMBERS`, `BOOM`, `COOLFNT1` (loaded for every island, `0x15b76` ff.) | any `ILE_n.COL` | the five island palettes are identical at indices 1–14, 128–170, 182–253 and 255, and these sheets use only such indices |
| `COOLFNT2`, `COOLFNT3` | `MENUSPIC.RON` | loaded with the menu at `0x25c07` ff. |
| `HAND` | `MARCHAND.RON` | loaded with the shop at `0x2a677` — pairing by call site, **hypothesis** |
| `INSTFNT1..3`, `COOLFONT` | `INSTALL.RON` | installer assets; trailing palettes agree on all used colours — **hypothesis** |

Checked against DOSBox-X captures:

- Island 1 gameplay frame: all 179 colours on screen are entries of `ILE_1.COL`, and
  `NUMBERS.SPT` sprites 0, 3, 6, 10, 11, 12, 16, 25 decoded with `ILE_1.COL` are found
  pixel-exact in the HUD (score digits, percentage, hearts, duck face, item slot).
- Main menu frame: `COOLFNT2` glyphs `M A I N E U` and `COOLFNT3` glyphs
  `L A D G M E O P T I N S X` decoded with `MENUSPIC.RON`'s palette are found pixel-exact in
  the `MAIN MENU` / `LOAD GAME` / `OPTIONS` / `EXIT` captions.

World sprites (duck, enemies, pickups) are drawn through the scaling blitters, so they do
not match pixel-exact; their colours do.

## Fonts — **verified** (`0x32bb0`, `0x32c10`, `0x32ce0`)

A font is an ordinary sheet selected with `set_font(sheet, first_char, spacing)` (`0x32bb0`).

- Glyph index = `byte - first_char`. There is no translation table. The game always passes
  `first_char = 0x20`.
- A byte without a glyph is skipped with no advance. (The range check at `0x32c3b` accepts
  `index <= count`, one too many.)
- Advance = low byte of the glyph's `width` + `spacing` (`0x32c54`). Calls seen:
  `set_font(COOLFNT1, 0x20, -5)` on the islands (`0x15b7e`), `set_font(COOLFNT2, 0x20, 0)` in
  the menu (`0x25c33`).
- Text can be left-aligned, centred or right-aligned at x (argument 0 / 1 / 2 of `0x32c10`).

| Sheet | Glyphs | Bytes covered | Use |
|---|---|---|---|
| `COOLFNT1` | 210 | `0x20`–`0xF1` | dialogue and in-game text, all five languages |
| `COOLFNT2` | 59 | `0x20`–`0x5A` | menu, orange, upper case only |
| `COOLFNT3` | 59 | `0x20`–`0x5A` | menu, green, upper case only |
| `INSTFNT1` | 144 | `0x20`–`0xAF` | installer |
| `INSTFNT2`, `INSTFNT3`, `COOLFONT` | 59 | `0x20`–`0x5A` | installer / unused |

The character set of `COOLFNT1` (read off the glyph images) is described in `txt.md`.
Glyphs without a picture are sprites of the full cell size whose rows are all empty (the space in every font; ``# $ & ' ( ) * + / < = > @`` in `COOLFNT2`; `0x7F`, `0x9D`–`0x9F`, `0xA9`–`0xAC` in `COOLFNT1`).
