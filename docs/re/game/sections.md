# "Sections": pictures, map, bonus items, boss fights, transitions

**verified** against the code at the cited addresses unless marked otherwise.

## 1. There are no vehicle or bonus stages

Every file that looks like a stage is a 640×480 still picture from `PICTURE.BNK`, shown by
one of four modal helpers. All of them: read input once, fade the game palette out, decode
the RON to the back buffer, blit, fade in, wait, fade out, clear, request a game fade-in
(`[0x40058] = 1`) and reset `tick_counter` so the pause does not produce a burst of ticks.

| Helper | Addr | Wait | Used for |
|---|---|---|---|
| `picture(name)` | `0x28750` | any gameplay input pressed, then released (fade step 10) | `RADEAU`, `ULM`, `RADAR`, `JETSKI`, `TOUR`, `BIPLAN`, `COMMANDO`, `BOMBE`, `EXPLOS02` |
| `picture_fade(name, speed)` | `0x28870` | same, with the given fade speed | `TRANS1-2` … `TRANS4-5` (speed 0x96), from `main` |
| `picture_timed(name, n)` | `0x28990` | timed (argument 16), no key | `EXPLOSE`, `EXPLOS01` (first two frames of the 3-picture explosion) |
| `text_on_picture(line, name)` | `0x290e0` | as `say`, with dialogue line `line` drawn on the picture | `MISS`/`CATCH` (island 3 marmot), `DEPART` (island 4 exit) |

`picture_timed(name, n)` (**verified**, `0x28990`): `n` is the number of palette fade steps
(fade out of the game, fade in of the picture, fade out again, each `n + 1` retraces); the
hold in between is a fixed `delay(100)` (Watcom, milliseconds). No key is read.

What each picture means in play:

- **Plans** (`RADEAU`, `ULM`, `RADAR`, `JETSKI`, `TOUR`, `BIPLAN`, `COMMANDO`): shown when the
  plan item is picked up and again on key `1` / `2` while it is owned. They are the
  player's checklist of parts (hypothesis about the content; not viewed).
- `BOMBE`: a bomb was assembled (island 5). `EXPLOSE`/`EXPLOS01`/`EXPLOS02`: a vessel was
  blown up.
- `TRANSn-m`: between islands. `DEPART`: leaving island 4. `GO_n`: game over picture of
  island n. `GAMEOVER.RON`, `THE_END.RON`, `INSTALL.RON`: not referenced by `P.EXE`
  (**verified**: no string reference; `THE_END.EXE`/`INST.EXE` are separate programs).
- `ANIMAPS.RON`: 192-pixel-wide sheet of animated terrain tiles (32×32 cells), copied into
  the terrain texture set by `0x11b40` every frame: 6 animated tiles, each with a 0..17
  countdown started at random (one `rand()` per frame for all six: `rand() % 20 == index`
  starts tile `index` if its counter is 0; this call is part of the gameplay random
  sequence), frame `(17 - t)/3`.
  Source/destination pairs at `0x10200`: (row 0 → tile 0), (1 → 1), (2 → 2), (0 → 3),
  (3 → 1498), (4 → 1501), (5 → 1497).
- `CIEL_n.RON`: sky strip. `DIAL_n.RON`: dialogue background. `INV_0n.RON`: inventory
  background. `MARCHAND.RON`: shop (see `items-and-scripts.md`). `MENUSPIC.RON`: menu.

## 2. Map screen (`0x280f0`, key Tab)

```
decode CARTE1n.RON to the screen (the "unexplored" picture)
decode CARTE0n.RON to the back buffer (the "explored" picture)
for cz in 0..239, cx in 0..255:                       ; explored[cx][cz] at 0x2456b0 + cx*256 + cz
    if explored[cx][cz]: copy the 2×2 pixel block at (2*cx, 2*cz) from the back buffer
for each teleporter marker i < 10 (x: 0x516750[i], z: 0x50da38[i], -1 = unused):
    3×3 block, colour 0xfe, at (x*2/300 + 255, z*2/300 + 243)
for each hint marker i < 20 (x: 0x50d948[i], z: 0x50d9c0[i]):
    3×3 block, colour 0xfd, same mapping
player: sx = 256 - (rx*2)/300, sy = 240 - (rz*2)/300        ; rx, rz are negated world coords
    without compass: 6×6 block colour 0xff at (sx-2 .. sx+3, sy-2 .. sy+3)
    with compass: triangle outline, tip trunc(7*cos(angle)), trunc(7*sin(angle)) from (sx, sy),
                  the two back corners at angle ± 0.7 (see 0x28486 for the exact expressions)
palette entries 0xfd = green (0,63,0), 0xfe = (0,63,0)/(63,..) and 0xff set just before:
    0x50d93f.. = {0,63,0}, {0,0,63}, {63,0,0}  (VGA 6-bit; order fd, fe, ff)
wait for input press + release; fade back
```

So one map pixel pair per terrain cell (2 px per cell, 512×480 of the picture). The
`explored` array (0x10000 bytes at `0x2456a8`, cleared at island start, saved in save
games) is written by the terrain renderer (`0x12aff` in `ground_build_vertices`,
**verified**): every frame it sets `explored[x][z] = 1` for the 30×30 cell window with
origin `(128 - cam_x/300 - 15, 128 - cam_z/300 - 15)` (`0x4f0ee0`, `0x4f0ee4`; `cam_x/z` =
the stored negated duck position, C division). The same window, as left by the previous
frame, is what the sea-volume count (`0x188e0`) scans. **Confirmed**: the port's explored
map is byte-identical to the original's after recorded runs.

The hint markers are added by `hint(id)` whenever the duck touches an item it cannot take
yet, so the map remembers where refused items are. Teleporter markers are added when a pad
is armed. Both lists only grow (first free slot; full lists ignore new entries).

## 3. Bonus items

See `entities.md` section 4. "BONUS" in file names and error strings means the pickups and
decor sprites on the island.

## 4. Boss fights

There is no arena or mode switch. The island script sets `boss_state = -1` after the last
build step; `monsters_update` then creates the boss in monster slot 0 **at world (0, 0)**,
the map centre, wherever the duck is. From then on:

- no monster respawns any more;
- the boss is always processed (not limited to the 6000-unit activity box), detects the duck
  from anywhere (detect radius² 0x168000·1000 → 38 400 units), walks toward it axis-wise at
  100 units per frame per axis with no terrain constraint, and attacks within
  `sqrt(4500·1000) ≈ 2121` units with up to 5 simultaneous shots
  (`fire_chance (diff*50+100)/256` per frame, shot speed `diff*50+150`, damage 5 per hit);
- HP `(diff+1)*island*20 + 150`; damage per player shot `(weapon+2)/2`; HUD bar scaled to
  `(diff+1)*island*10 + 101`;
- island 5: it has to be killed three times (re-created at (0,0) with full HP);
- when its death animation ends `boss_state = 2`; the script then shows the exit message
  for the difficulty and, on medium/hard, waits until the monster count is below
  half / below 2.
- If the duck walks onto the build site while the boss is alive the script shows the
  "must finish the mission" line once (re-armed whenever the duck stands on zone 0/1).

Boss sprites: `BOSSn.SPT`, table row "boss" in `entities.md`, drawn at scale 12.0.

## 5. Ending

`next_island == 6` → CD track 7, fade out, `PART_1.MOV` then `PART_2.MOV` (FLC player
`0x2bc70`, second argument 15 and 1), back to the main menu. `DUCK.BAT` then runs
`THE_END.EXE` after `P.EXE` exits. See `overview.md` for the island-1 shortcut.
