# `OBJETS_n.MAP` — texture page of the 3D objects

Parser: `crates/pd-formats/src/objmap.rs`. Dump: `objets_n_page.png`, `objets_n_tex_KK.png`.

## Layout — **verified** (loader `0x141a0`, second half from `0x143fb`)

| Offset | Size | Loaded to | Content |
|---|---|---|---|
| 0 | 4 | `0x4f0e54` | `rows` (u32), page height in pixels |
| 4 | 4 | `0x4f0e58` | `count` (u32), number of textures |
| 8 | `0x300` | `0x1cbba8` | palette, 6-bit VGA — read, then overwritten by the next read |
| `0x308` | `count * 12` | `0x1cbba8` | texture records |
| … | `rows * 256` | `0x48aac0` | page pixels, 256 wide, 8-bit indexed |

Sizes add up exactly for all five files. **verified**

Texture record: `u32 offset, u32 width, u32 height`. `offset` is the byte offset of the
texture's top-left texel in the page; it is the only field the game reads
(`0x14f0b`: texture base = `0x48aac0 + record[texture].offset`). **verified**
Width/height: every rectangle `offset%256, offset/256, width, height` frames exactly one
picture on the page. **verified** by output.

A face addresses a texel as `page[offset + v * 256 + u]`; see `3d.md`.

| Island | rows | textures |
|---|---|---|
| 1 | 416 | 19 |
| 2 | 230 | 29 |
| 3 | 264 | 21 |
| 4 | 342 | 12 |
| 5 | 309 | 15 |

The pixels use the island palette (`ILE_n.COL`). Whether colour 0 is transparent in the object
rasteriser (`0x2c838`) was not checked.
