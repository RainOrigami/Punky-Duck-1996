# `ILE_n.COL` — island palette

Parser: `crates/pd-formats/src/col.rs`. Dump: `ile_n_col.png` (16x16 swatches), `ile_n_col.json`.

## Layout — **verified** (loader `0x33440`)

| Offset | Size | Content |
|---|---|---|
| 0 | 4 | u32 776 (file size) |
| 4 | 2 | u16 `0xB123` |
| 6 | 2 | u16 0 |
| 8 | 768 | 256 x RGB, 8 bits per component |

The loader reads the 8 header bytes into the destination and overwrites them with the 768
palette bytes, so the header is not checked. Every component is then divided by 4
(`0x33490`) to give the 6-bit VGA DAC value. The header matches the Autodesk Animator Pro
`.COL` file header (size, magic `0xB123`, version 0) — **hypothesis** as to origin, the
bytes themselves are verified.

## Use — **verified**

Called at `0x12249` (and the four sibling blocks) with destination `0x4f0b00`, right after
`Animaps.RON` and `Ciel_n.RON` are loaded with the same palette destination. It is the
palette of island gameplay; `ILE_n.UNI` and `OBJETS_n.MAP` carry 6-bit copies of the same
colours. It is not a remap or shade table (those are in the UNI).
