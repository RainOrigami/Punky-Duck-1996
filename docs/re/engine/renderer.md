# Renderer (P.EXE)

**Verified** in disassembly/decompilation at the cited addresses unless marked.

## Summary — what the island view actually is

The island is **not** a per-scanline "Mode 7" floor. It is a true 3D scene:

- a **30×30-vertex height-field mesh** (29×29 cells, each split into two triangles)
  around the camera, vertices taken from a 256×256 height table, projected with a
  floating-point look-at camera (`project_points`, `0x100be`);
- each triangle drawn with an **affine texture mapper** (8.8 fixed-point UV, 32×32 tile
  textures, 4 rotations), cells painted **back to front** (depth sort, no z-buffer);
- a second pass per triangle applies **depth fog** through a 32-level colour remap table;
- textured 3D objects (`.3D`/`.MAP`) and scaled sprite billboards are interleaved with
  the cells by depth;
- the sky (`Ciel_n.RON`) is copied row by row above the horizon line.

On flat ground this looks like a Mode-7 plane, which is what a capture shows; the code
has no per-scanline floor math. Water cells are animated by moving their vertices.

## Video modes and presentation

- Menu and stills: VESA mode **0x101** (640×480×256) through VBE 1.x **bank switching**
  (`vesa_init` `0x31c60`, `vesa_set_mode` `0x323d0`, `vesa_set_bank` `0x323ed`); no
  linear frame buffer. Everything is composed in the memory back buffer `0x4a5ae0`
  (pitch 640) and copied with `vesa_blit` (`0x32708`).
- Island view, mode in `0x4002c` (F1 cycles; `view_mode_apply` `0x146f0`):

  | Mode | Render size | Presented |
  |---|---|---|
  | 0 | 320×200 | BIOS mode 13h, `blit` to `0xA0000` |
  | 1 | 320×200 | mode 0x101, at (160, 80) |
  | 2 (default) | 640×400 | mode 0x101, at (0, 40); with F2 "interlace" (`0x40044`/`0x40054`) only even rows are rendered and presented |

  Clip rectangle (`set_clip`): (0,0)–(319,199) or (0,0)–(639,399). Border colour 0xFE.
- `blit` (`0x305c4`) and `vesa_blit` take a byte count and a row count, but the game
  passes `0x13f, 199` / `0x27f, 399` (and `0x1f, 0x1f` for animated tiles): **the last
  column and last row are not copied** (639×399 presented; **verified** by capture: column
  639 and row 439 of the screen keep the border colour). Port: `Renderer::present`,
  fix behind `Enhancements::present_full_view`. The triangle fillers additionally never
  draw the row `clip_y1` (399 / 199), see "Triangle fillers".
- The back buffer is never cleared between frames; `view_mode_apply` fills it (and the
  screen) with `0xFE`. Sky rows are copied 639 (319) bytes wide, so column 639 of the sky
  keeps stale content. With interlace on, odd screen rows keep the `0xFE` fill
  (**verified** by capture: F2 goes through `view_mode_apply`).
- `render_frame` (`0x148a0`) order: `camera_update`, `world_sprites_sort`,
  `objects3d_transform`, `ground_build_vertices`, `sky_draw`, `ground_draw`, `hud_draw`,
  then present. With `0x40058 == 1` it first runs `view_mode_apply` (fade to black, mode
  switch) and presents nothing that frame; state 3 → 2, and at 2 it fades in over 20
  steps from black (`0x197dd0`) to the island palette (`0x4f0b00`) and zeroes `g_ticks`.

## Palettes

6-bit VGA DAC triples, 768 bytes. `0x4f0b00` island palette (from `ILE_n.UNI`, then
overwritten by `ile_n.col`), `0x197dd0` all black, `0x489690` palette of `Animaps.RON`,
`0x50cbc4` menu. `palette_set` `0x331a0`, `palette_fade` `0x331f0`
(`from + (to - from) * i / steps`, i = 0..steps, one vertical retrace per step, signed
integer division). `load_ron` and `col_load` shift 8-bit palettes right by 2.

## World data used by the ground renderer (from `ILE_n.UNI`, see `uni_load` `0x12700`)

| Global | Size | Content |
|---|---|---|
| `0x4f0b00` | 0x300 | palette |
| `0x485750` | 0x2000 | fog table: 32 levels × 256, `out = table[level*256 + colour]` |
| `0x2c4498` | `nblocks*256` | tile texture atlas, **256 bytes per row** |
| `0x487750` | `noffsets*4` | per-tile offset into the atlas → pointer to the tile's top-left texel; a tile is 32×32 inside the 256-wide atlas |
| `0x2a4498` | 0x20000 | 256×256 u16 tile map: bits 0–11 tile index, bits 13–14 rotation (0–3); bits 12 and 15 are ignored by the renderer (`and eax,0xfff` at `0x12d89`, `sar eax,0xd / and eax,3` at `0x12db7`) |
| `0x22cdc8` | 0x10000 | 256×256 u8 cell type; the renderer only tests 1 and 13 (waves) and 6 (double-amplitude waves) |
| `0x261b88` → `0x1f2668` | 0x10000 → 256×256 i16 | height bytes, converted at load to i16 `trunc(byte * 3.0)` (`0x1283f`, f64 const `0x423ef` = 3.0), raw bytes then zeroed (buffer reused as sprite scratch) |

Map indexing is `[a*256 + b]` (u16/i16 tables `[a*0x200 + b*2]`) with
`a = clamp((0x80 - cam_cell_x) - 15 + i, 0, 255)`, `b = clamp((0x80 - cam_cell_z) - 15 + j, 0, 255)`.
Cell size is **300 world units** (`0x4f0ec0 = [0x126e8]=15 × [0x126e4]=20`); grid side
`[0x126ec] = 30`, vertex count `[0x126f0] = 900`.

## Camera — `camera_update` (`0x2b1c0`)

Inputs: player position ints `0x517f8c` (x), `0x517f90` (y), `0x517f94` (z), heading
float `0x517fa0` (radians), altitude int `0x517ed8`.

```
cell = 300
cx = x / cell (0x4f0ed4)   fx = x % cell (0x40064)        // 0x133d0, C signed div/mod
cz = z / cell (0x4f0edc)   fz = z % cell (0x40068)
c = cos(heading), s = sin(heading)
grid_ox = trunc(c*1410 + cx)   (0x4f0ec8)                 // doubles at 0x42fb4 = 1410.0
grid_oz = trunc(s*1410 + cz)   (0x4f0ed0)                 // every "call 0x305a4; fistp" truncates
eye_h_target = altitude/300 + 250  (0x4006c)   look_h_target = altitude/300 + 100 (0x40070)
eye_h  (0x40074) += (eye_h_target  - eye_h ) / 5          // integer division, per frame
look_h (0x40078) += (look_h_target - look_h) / 5
eye    = (float cx, float eye_h, float cz)                 // 0x4f0f30, 0x4f0f34, 0x4f0f2c
target = (trunc(cx + c*1375), look_h, trunc(cz + s*1375))  // 0x4f0f24, 0x4f0f28, 0x4f0f20; 0x42fbc = 1375.0
camera_build_matrix()
base_x (0x4f0e28) = float(grid_ox + fx - 15)
base_z (0x4f0e2c) = float(grid_oz + fz - 15)
```

i.e. the scene is built in a camera-local frame whose origin is the eye's *cell index*;
the grid (and thus the player) sits 1410 units in front of the eye along the heading,
the eye looks at a point 1375 units ahead, eye height 250 and look height 100 above
`altitude/300`, both smoothed by 1/5 per rendered frame (frame-rate dependent).

Ground vertex (i = row 0..29 along z, j = column 0..29 along x), `ground_build_vertices`
`0x128c0`:

```
vx = float(grid_ox + fx - 15*300) + j*300
vz = float(grid_oz + fz - 15*300) + i*300
vy = height[a][b]                                   // i16 → float
     (+ wave[(a*128 + phase) & 511]      if type is 1 or 13)
     (+ wave[...]*2                      if type is 6)
phase (0x4f0e14) += 16 per frame, +50 per row;  wave[k] = 512-entry table 0x1cb3a8,
     trunc(cos(k * 3.1415 * 2.0 * (1/512)) * 5.0) // f64 consts 0x423cf, 0x423d7, 0x423df, 0x423e7
wave index = (a*128 + row_phase) & 511 with a = the cell's first (x) map index; row_phase
     starts at `phase` and gets +50 *before* each row is built.
```

## Projection — `project_points` (`0x100be`) and `camera_build_matrix` (`0x152d0`)

Matrix build:

```
f = normalize(target - eye)                 // 0x4f0f64 (x), 0x4f0f60 (y), 0x4f0f38 (z)
d = 5.0                                     // 0x4f0f44
if clip_x1 > 330: Sx = 120, Sy = 432, Cx = 320, Cy = 240   // hi-res; 0x4f0f6c/74/68/70
else:             Sx =  60, Sy = 216, Cx = 160, Cy = 100
yaw = atan2-like(f.z, f.x)                  // helper 0x15230(f.z, f.x)
r = ( sin(yaw + PI/2), 0, cos(yaw + PI/2) ) // PI/2 const 1.57079632675; |component| < 1e-10 → 0
u = f × r   = ( f.y*r.z - f.z*r.y,  f.z*r.x - f.x*r.z,  f.x*r.y - f.y*r.x )
R = Sx * r   (0x4f0f18, 0x4f0f14, 0x4f0f10)
U = Sy * u   (0x4f0f50, 0x4f0f4c, 0x4f0f48)
O = -d * f   (0x4f0f58, 0x4f0f5c, 0x4f0f54)
```

Per point `P` (3 × f32), outputs `sx`, `sy` (i32) and `depth` (i16):

```
v     = P - eye
depth = v · f                 → stored with fistp (round to nearest) as i16
q     = v * (d / depth) + O   // point on the image plane, relative to its centre
sx    = round(Cx - q · R)
sy    = round(Cy - q · U)
```

All x87 single-precision stores, default rounding (nearest-even) for `fistp`. There is
no near-plane clipping of geometry: `depth <= 0` just produces garbage coordinates that
the bounding tests below reject (triangles are dropped when they straddle the whole
screen, `min < 0 && max > clip_max`). Equivalent focal lengths in hi-res: 600 px
horizontally (`Sx*d`), 2160 vertically (`Sy*d`) — the vertical axis is in different
units (heights are small numbers: eye at 250 above ground vs. 300 per cell… the same
units as x/z *divided by* the height scale; treat the constants as given).

## Ground drawing — `ground_draw` (`0x14b60`), `ground_draw_triangle` (`0x12bd0`)

- Cells are sorted by the depth of their first vertex (quicksort `0x158c0` on i16 keys,
  index list `0x296460`) and drawn far → near. A cell is skipped if it is in the last
  row/column, or if not (`max depth of its 4 vertices > [0x40038]=0` and
  `min depth < [0x40034]=6000`).
- Cell with vertex index `n = i*30 + j`: triangles `(n, n+1, n+30)` (half 0) and
  `(n+31, n+1, n+30)` (half 1), tile word from the cell's map entry.
- `ground_draw_triangle`: reject if outside the clip rectangle in x or y, or if it spans
  the whole screen with a negative minimum; **back-face cull** by the sign of the 2D
  cross product (half 0 needs ≥ 0, half 1 needs ≤ 0); texture pointer =
  `tile_ptr[tile & 0xfff]`; UV corners are 0 or 31 according to rotation
  `(tile >> 13) & 3` and half (table in the code at `0x12d5a`–`0x12f3f`).
- `tri_textured_opaque` (`0x2da30`): sorts the three vertices by y, walks edges in 16.16
  fixed point, per span affine UV in 8.8 (`texel = tex[(v>>8)*256 + (u>>8)]`), clipped
  to the clip rectangle; vertices in `0x4a5ac0..c8` (x), `0x4a5ad0..d8` (y), UVs in
  `0x4f0af0..f8` (u), `0x4f0ae0..e8` (v). Span code is self-modifying (`0x2e0e4`).
- Fog pass (only if `[0x40030] != 0`, default 1500): per vertex
  `level = (depth - ([0x40034] - [0x40030])) * 50 / [0x40030]` = `(depth - 4500)*50/1500`
  → `0x4f0e00..08`; `tri_fog` (`0x2d158`) interpolates the level across the triangle and
  rewrites each pixel as `fog[level][pixel]` (levels < 0 leave pixels untouched, top
  level 31; details under "Triangle fillers"). Far limit 6000, fog starts at
  4500 depth units.
- Draw order quirk: the sorted index list is walked from entry 900 down to 1
  (`0x296460[900 - k]`, k = 0..899), so the vertex with the smallest depth key (entry 0)
  never starts a cell. Ties are ordered by the quicksort at `0x158c0` (Hoare partition
  around the middle element, recursion on the left part, loop on the right).
- After each cell, pending 3D-object faces and world sprites whose depth is greater than
  the cell's maximum depth are drawn (painter's interleave).

## 3D objects

`objets3d_load` (`0x141a0`) fills: models `0x271b88` (16 bytes: first/last point, first/
last face), points `0x23d368` (3 × f32, scaled ×2.0 at load), faces `0x1cc508` (24 bytes:
i16 texture id at +0, 3 × i16 point indices at +6/+8/+10, 3 × i16 u at +12, 3 × i16 v at
+18), texture table `0x1cbba8` (12 bytes, u32 offset first), texels `0x48aac0`
(256-wide pages). `object3d_place` (`0x14050`) creates up to `[0x4005c]` instances
(72 bytes at `0x23cdc8`: active, model ranges, position (x + 128 cells, y/300, z + 128
cells), 3 rotation angles, flag), culled when farther than 4500 units from the player
in x or z. `objects3d_transform` (`0x13c40`) rotates (`0x156e0`, `0x15790`), projects and
depth-sorts faces; faces are back-face culled and drawn with `tri_textured_keyed`
(`0x2c838`, texel 0 transparent) when nearer than `[0x4003c] - 1500` = 6500, followed by
a fog pass with factor 64 instead of 50 (`0x14ea0` area).

Rotation (`0x156e0(a, b, c)`, matrix `0x4f0eec..0x4f0f0c`, applied by `0x15790`):
`R = Rz(c) · Ry(b) · Rx(a)` — first `a` about the x axis (`y' = c1*y - s1*z`), then `b`
about the y axis (yaw), then `c` about z. `objects3d_transform` calls it as
`0x156e0(inst.rot[0], inst.rot[1], 0)` (call at `0x13dac`): the instance's third angle is stored
but **never applied**. Then `world = (pos.x + x', pos.y - 0.24 * y', pos.z + z')`.

Per frame: every slot's position is projected first; an instance is dropped when its
depth is `< 401` or `>= 6000` or its screen x is outside `clip ± 300`. Faces of the
remaining instances are keyed by `(d0 + d1 + d2) / 3 - 100` and sorted like the cells.
A face is drawn after a cell when its key exceeds the cell's maximum depth, if
`(x1-x0)*(y2-y0) - (y1-y0)*(x2-x0) > 0`, its bounding box touches the clip rectangle and
is less than 1501 pixels wide and high. There is no near clipping: faces with a vertex
behind the eye produce huge skewed triangles (**verified** by capture, hut seen from
inside matches pixel for pixel).

## World sprites (billboards) — `world_sprite_draw` (`0x13500`)

World sprites do **not** use the scaled sprite blitters; they are unpacked and drawn as
two textured triangles.

`world_sprite_add(index, bank, flags, x, y, z, angle, scale, anchor)` (`0x138f0`) appends
a 44-byte entry at `0x489990` (max 99; dropped if farther than 4500 units from the
player in x or z): `+0 bank base`, `+4 visible`, `+8 x + 128*300`, `+0xc y/300`,
`+0x10 z + 128*300`, `+0x18 sprite index`, `+0x1c flags`, `+0x20 angle (f32)`,
`+0x24 scale (f32)`, `+0x28 anchor`. `0x130f0` projects them (if `y < 0` the height is
`ground_height_at(x,z) - y`) into `sx 0x29b8c0`, `sy 0x29f290`, depth `0x29f420`;
`0x13300` sorts them by depth. `ground_draw` draws a sprite when its depth exceeds the
current cell's, if depth < 6000 and `-40 < sx <= clip_x1 + 40`, with

```
k = |sy - sy_ref| * entry.scale / 200.0          // sy_ref (0x1c6ee8): the same point projected again with y + 60.0
                                                 // (0x132af adds the f64 0x423f7 = 60.0 to every sprite point)
if entry.scale < 0:
    if entry.scale > -900:  k = {0.5, 0.5, 1.0}[view_mode]     // fixed size (table 0x126f4)
    else:                   horizon_row (0x4f0e64) = sy; k = 0  // horizon marker, not drawn
```

`world_sprite_draw(index, bank, sx, sy, angle, k, anchor, flags, depth)`:

1. `level = (depth - 4500) * 72 / 1500`; if `level >= 32` the sprite is skipped.
2. `sprite_unpack` (`0x2d940`) decodes the SPT sprite into the 256-wide scratch page
   `0x261b88` (colour 0 = empty; row `r` lands on scratch row `r + 1`).
3. If `level >= 0`, every row is remapped in place through `fog[level]` (`0x306dc`,
   skipping zero pixels). If `flags & 8`, every non-zero pixel becomes `0xFF`
   (`0x30745`, silhouette/flash).
4. Corner offsets `(ox, oy)` in sprite pixels by anchor: 0 = from the SPT offsets
   (`-off_x .. w - off_x`, `-off_y .. h - off_y`), 1 = centred (`±w/2`, `±h/2`),
   2 = bottom-centre (`±w/2`, `-h .. 0`).
5. Each corner → screen: `X = sx + k*(ox*C - oy*S)`, `Y = sy + k*(ox*S + oy*C)` with
   `C = cos(angle)` (`[ebp-8]`), `S = sin(angle)` (`[ebp-4]`), truncated (`0x13750`);
   `flags & 1` selects the mirrored variant (`0x137b5`): `X = sx - k*(ox*C - oy*S)`, Y
   unchanged. Corners are 0 top-left, 1 top-right, 2 bottom-right, 3 bottom-left; for
   anchors 1 and 2 `sx`/`sy` are first moved by `(w/2 + hot_x, h/2 + hot_y)` resp.
   `(w/2 + hot_x, h + hot_y)`.
6. Two `tri_textured_keyed` calls: corners 0,1,3 with UV `(0,0) (w,0) (0,h)` and corners
   1,2,3 with `(w,0) (w,h) (0,h)`.

Details found while porting (**verified** in code, and by capture where noted):

- The fog remap and the flash loop start at page row 0 and run `h` rows, while the sprite
  occupies page rows 1..h: the **last sprite row is never fogged or flashed**. Page row 0
  is never written (always 0 = transparent) and is what `v = 0` samples.
- Sprite points use `base_x = grid_ox + frac_x - 15` (`0x4f0e28`), ground vertices use
  `grid_ox + frac_x - 15*300 + j*300`: sprites and 3D objects sit 15 units off the ground
  grid in x and z.
- `y >= 0` in the queue entry is an absolute height (`y/300`, same units as the i16 height
  table), `y < 0` means bilinear ground height plus `|y/300|`.
- The horizon marker (`scale <= -900`; `player_draw` queues DUCK frame `0x2d` 3000 units
  ahead at height 0 with scale −999) is still passed to `world_sprite_draw` with `k = 0`;
  all four corners coincide and nothing is drawn. It sets the sky's horizon row for the
  **next** frame (the sky is drawn before the ground). **verified** by capture: sky rows
  match with horizon 171 / 170 / 126 / 151 computed this way.
- Sort key is `depth + 1500`; a sprite is drawn after the first cell (far → near) whose
  maximum vertex depth is below the sprite's depth, after that cell's pending object faces.

## Triangle fillers (asm, `0x2c838`–`0x2e34f`)

Common interface (globals): x `0x4a5ac0/c4/c8`, y `0x4a5ad0/d4/d8`, u `0x4f0af0/f4/f8`,
v `0x4f0ae0/e4/e8` (ints, texel units), texture page pointer `0x4f0ee8` (256 bytes per
row). Vertices are sorted by y; edges stepped in 16.16; spans clipped to the clip
rectangle; UV stepped in 8.8 per pixel: `texel = page[(v >> 8) * 256 + (u >> 8)]`.
In interlace mode (`0x40054`) odd rows are skipped.

| Routine | Span loop | Pixel rule |
|---|---|---|
| `tri_textured_opaque` `0x2da30` | `0x2e0e4` | every texel written (ground) |
| `tri_textured_keyed` `0x2c838` | `0x2ceec` | **texel 0 is transparent** (`or al,al / je` at `0x2d0a7`) — 3D objects and world sprites |
| `tri_fog` `0x2d158` | `0x2d717` | no texture: interpolates the per-vertex level `0x4f0e00/04/08` and rewrites `dst = fog[level*256 + dst]` |

Exact rules (ported 1:1 in `crates/pd-game/src/render/raster.rs`, **verified** by capture):

- Vertex sort: three compares (`0x2da42`), the last one swaps vertices 1 and 2 also when
  their y are equal.
- Reject when `y0 > clip_y1` or `y2 < clip_y0`. Edges start at `x << 16 + 0x8000`, slopes
  are `((dx) << 16) / dy` with `cdq; idiv` (truncating).
- General case: long edge slope `m02`, short `m01`; span width at `y1` =
  `((m02 - m01) * (y1 - y0)) >> 16`; UV at the long edge at `y1` =
  `(uv2 - uv0) * (y1 - y0) / (y2 - y0) + uv0` (whole texels). If `m02 > m01` the width
  gets `+1` before the per-pixel gradient `((uv_long - uv1) << 16) / width` is computed
  (fog: `+3`, and `-2` in the other orientation); zero width drops the triangle.
- Rows: a span of `rows` starting at `y` draws `y .. y+rows-1`, clipped at the top by
  advancing the accumulators, at the bottom by `rows -= y + rows - clip_y1`: **row
  `clip_y1` is never drawn**.
- Pixels of a row: `xl = L >> 16`, `xr = R >> 16`, clipped to the clip rectangle
  (left clip advances UV by the 16.16 gradient). Nothing is drawn unless `xr > xl`;
  otherwise pixels `xl .. xr-1` get texels stepped in 8.8 (`gradient >> 8`), and **pixel
  `xr` gets a copy of the last texel fetched** (`mov [edi],al` at `0x2e2ff`; keyed
  variant: only if that texel is non-zero, `0x2d119`). The fog span rewrites `xl ..= xr`.
- Texel address: `page[((v >> 8) & 0xff) * 256 + ((u >> 8) & 0xff)]` of the 8.8 values.
- `tri_fog`: vertex levels are clamped to 0..31 on entry; if all three were negative the
  triangle is skipped; also skipped when entirely left or right of the clip rectangle.
  **Inside the span loop the level byte is not clamped** (`mov ch,ah`, `0x2d85b`): an
  interpolated level outside 0..31 indexes memory after the shade tables (tile pointer
  table `0x487750`) or, wrapped to 0xFE/0xFF, the object texture page (`0x48aac0`). In the
  five captured views this never happens (port counts it: `FrameInfo::fog_overruns`).
  Shade level 0 is applied to every triangle that has at least one vertex at depth > 4470
  (C division truncates `(depth - 4500) * 50 / 1500` to 0 for −29..−1).

Distance → shade level (all with fog start `F0 = [0x40034] - [0x40030]` = 4500 and
range `[0x40030]` = 1500; level index into the 32 tables of the UNI):

| What | Level | Where |
|---|---|---|
| ground triangle vertex | `(depth - 4500) * 50 / 1500` | `0x12f44` |
| 3D object face vertex | `(depth - 4500) * 64 / 1500`, only for faces nearer than 6500 | `0x14b60` |
| world sprite (whole sprite) | `(depth - 4500) * 72 / 1500`, not drawn at ≥ 32 | `0x13509` |

So shading is purely distance fog starting at depth 4500; nothing nearer is shaded,
and there is no lighting. Depth is the i16 view-space depth from `project_points`.

## Sky — `sky_draw` (`0x144a0`)

`scroll = (trunc(heading * 100000.0) % 314150) * 200 / 100000 + 6400` (`0x4f0e68`; f32
const `0x4245f` = 100000.0, C `%` and `/`) is added to the source pointer of the 640-wide
`Ciel_n.RON` buffer (`0x1980d0`, 640×300 on disc): the picture is read from row 10 on,
shifted by up to 628 bytes (heading is ≤ 0, so −628..0); bytes shifted past a row end
come from the neighbouring row. Each row copies 639 bytes (319 in the 320 modes, which
take every second sky row but **no horizontal reduction**: they show the left half of
the picture at full size). Horizon row `H = [0x4f0e64]` (set from a projected
far sprite, default 200). Hi-res: rows `H-340 .. H-60` copy sky rows `0..280`, rows
`H-60 .. H+10` are filled with colour `0x40`. Lo-res: rows `H-170 .. H-30` copy every
second sky row, rows `H-30 .. H+5` filled with `0x40`. The ground is drawn over it.

## 2D layer

- Sprites: `sprite_draw` `0x340e8`, mirrored `0x34468`; SPT layout in `functions.md` §1.
  Scaled variants `0x34558` / `0x350ce` are dead code, see below. Target/pitch/vertical clip via `sprite_set_target`
  (`0x33600`); bank selected by writing its base to `0x51f484`. No horizontal clipping.
- Text: `font_set` `0x32bb0`, `text_draw` `0x32ce0` (fonts are SPT banks, glyph =
  `char - first_char`, pen advance = glyph width + spacing; `COOLFNT1` uses first char
  0x20 and spacing −5 in game, `COOLFNT2` spacing 0 in menus).
- Primitives on the draw target: `hline` `0x30f3b`, `vline` `0x30fc9`, `fill_rect`
  `0x310bb`, `rect_outline` `0x3102d`; on the VESA screen: `vesa_fill_rect` `0x32a04`,
  `vesa_putpixel` `0x3240a`.
- FLC playback: `flc_play` `0x2bc70` (decoder `0x2c1e0`, chunk jump table `0x2c1a4`),
  decodes into `0x4a5ae0` and presents the whole 640×480.
- Animated tiles: `animaps_tick` `0x11b40` copies 32×32 frames from `Animaps.RON`
  (`0x21c5c8`, pitch 192: 6 animations × frames) into tile textures listed in the
  20-int table at `0x10200` (pairs: animation index, tile index); each animation is
  triggered at random (`rand() % 20 == k`) and runs 17 frames, frame = `(17 - t) / 3`.

## Scaled sprite blitters `0x34558` and `0x350ce` — unreferenced

Both are reached only from the sprite-list renderer `0x33800` (68-byte entries at
`0x51e470`), which has **no caller and no pointer fixup** in P.EXE; the whole
list/scaled path is unused library code. For completeness (**verified** code reading):
`f(index, x, y, sx, sy)` with 8.8 factors clamped to ≤ `0x100` (shrink only);
offsets/size are scaled (`off*s >> 8`); vertically an 8.8 accumulator adds `sy` per
source row and a row is emitted only when its integer part changes (otherwise skipped
via `row_size`); horizontally each run starts at `x_run * sx` (8.8) and every source
pixel is written to `dst[acc >> 8]` with `acc += sx` (nearest-neighbour by overwrite,
opaque runs, run length taken from the low byte only). `0x350ce` is the mirrored twin
(uses `x_mirrored`, `acc -= sx`, start x `+1`).

## Verification of the port against the original (**verified** by capture)

Port: `crates/pd-game/src/render/` (`Scene` → `Renderer::render` → `present`), headless
tool `pd-extract scene`, tests `crates/pd-game/tests/render_island.rs`. Ground truth:
DOSBox-X lossless captures of P.EXE loading edited copies of
`out/ref/island1_start_SAVEGAME.__0` (position/heading patched at offsets 4, 112, 120, 124,
132; menu keys `down enter enter down enter`). Compared per pixel over the 640×400 view;
the only differing pixels are the HUD and sprites that were not put in the test scene
(monsters) or whose animation frame differs.

| View | px, pz, heading | Differing pixels outside HUD/monster/animated-pickup boxes |
|---|---|---|
| island 1 start | −19650, −10575, 0 | 0 of 192 900 |
| start, turned | −19650, −10575, −0.9 | 0 (all 7 diff clusters are HUD, 2 animated pickups that match exactly at animation phase 3 resp. 4, 1 monster) |
| huts (3 meshes, one clipped by the eye, compass pickup) | −16500, −3600, 0 | 0 of 225 270 |
| water's edge, wave phase 64 of 32 tried | −10350, 21150, 0 | 0 (clusters: HUD, 3 sprites) |
| huts, view mode 1 (320×200 at 160,80), mode 0 (13h), mode 2 + interlace | as above | 0 outside HUD and the two sprites |

## Unknown / not decoded

- What the fog span reads for out-of-range levels beyond the tile pointer table and the
  object page (stale BSS); never reached in the captured views.
- Not exercised against a capture: sprite rotation (`angle != 0`), mirror and flash flags,
  anchors 1 and 2, object rotation angles, attribute 6/13 waves, islands 2–5, the camera
  while its heights are still settling.
- The 0x10000-byte block at `0x22cdc8` beyond values 1, 6, 13 is gameplay (see
  `game/player.md`).

- Turned view: the static difference cluster at x 308-364, y 272-337 is a monster (absent from the test scene). Confirmed by eye, so that view has no unexplained differences.
