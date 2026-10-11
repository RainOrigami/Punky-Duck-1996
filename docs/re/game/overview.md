# Game overview (gameplay level)

Source: static analysis of `P.EXE` (addresses are link-time addresses, identical in
`disc/extracted/P.elf`), plus the data files. **verified** = read in the disassembly /
Ghidra decompilation at the cited address. **hypothesis** = inferred.

Verified by a DOSBox-X run where noted (save game written by the original and parsed:
`out/ref/island1_start_SAVEGAME.__0`).

Related: `player.md`, `entities.md`, `items-and-scripts.md`, `sections.md`,
`save-and-settings.md`, `open-questions.md`, and `globals.md` (name ↔ address table used by
all of these).

## What kind of game this is

- Third-person 3D action/adventure on a height-mapped island. The camera sits behind the
  duck; the island is a 256×256 cell terrain (cell = 300 world units) rendered in
  perspective; duck, monsters, pickups and decor are billboard sprites placed in 3D;
  buildings / vehicles are textured meshes from `OBJETS_n.3D`. **verified** (terrain sampler
  `0x13010`, sprite queue `0x138f0`, mesh queue `0x14050`, camera `0x2b1c0`; also seen in the
  DOSBox captures described in `reference.md`).
- There are **no separate gameplay modes** for the raft, ULM, jetski, biplane, radar or
  bonus stages. `RADEAU.RON`, `ULM.RON`, `JETSKI.RON`, `RADAR.RON`, `TOUR.RON`, `BIPLAN.RON`,
  `COMMANDO.RON`, `BOMBE.RON`, `CATCH.RON`, `MISS.RON`, `DEPART.RON`, `EXPLOS*.RON`,
  `TRANSn-m.RON` are all **full-screen still pictures** shown by `show_picture` (`0x28750`,
  `0x28870`, `0x28990`, `0x290e0`) and dismissed with any input. `BONUS_n.SPT` is the sprite
  bank of the pickups and decor objects standing on the island, not a bonus stage.
  **verified** (every reference to these names is an argument of those four functions).
- Each island is one fetch-and-build quest: find the plan, collect parts in a fixed order,
  bring them to a build site, kill the boss that appears, (on higher difficulty) thin out
  the monsters, return to the build site, leave. The quest logic is **hard-coded C per
  island** (one function each), driven by a single step counter. See
  `items-and-scripts.md`.

## Islands

Names from the level-select menu table at `0x41920` (**verified**). Player start is the
value written to `player.px/pz` by the island start function (stored negated, see
`player.md`); island 1's (−19650, −10575) is confirmed by the original's own save file
(`px`, `pz`, `start_x`, `start_z` fields).

| n | Menu name | Objective (from `ILE_n.TXT` + script) | Start `(px,pz)` | Slide `S` | Script | Start fn | Asset fn |
|---|---|---|---|---|---|---|---|
| 1 | FISHER ISLAND | build a raft: plan, liana+nails, 2 planks, trunk (mast), sail | (-19650, -10575) | 8 | `0x178d0` | `0x17680` | `0x174c0` |
| 2 | OASIS ISLAND | build an ULM: plan, hatchet, wood/box/stolen item, wrench+bolts+wheels, pedals, wings, engine, propeller | (25087, 20362) | 8 | `0x19300` | `0x19030` | `0x18e70` |
| 3 | YETI ISLAND | build a shelter + radar (sheets, riveter, cable, dish, trap a marmot for power), then a jetski (5 parts) | (8100, 19387) | 15 | `0x1ae00` | `0x1aa80` | `0x1a8c0` |
| 4 | COLD ISLAND | cut ice blocks, build the tower, flatten the runway, build the hang glider/biplane | (-24412, -25387) | 40 | `0x1cae0` | `0x1c760` | `0x1c5a0` |
| 5 | SMOKE ISLAND | get a commando outfit (3 parts), build 4 bombs (3 parts each) and blow up 4 vessels | (-4050, -22500) | 8 | `0x160d0` | `0x15ce0` | `0x15b10` |

`S` is the position-smoothing divisor `player.slide` (`0x517f84`); bigger = more inertia
(island 4 is ice). **verified** (asset fns).

Per island files (digit patched into the name): `ILE_n.UNI` (terrain: palette, textures,
height map, zone map; loaded by `0x12700`), `ILE_n.COL` (colour remap), `ILE0n.TRJ`
(pickup/decor placement), `MONSTERn.TRJ` (monster placement), `OBJETS_n.3D/.MAP`,
`ENEMYn.SPT`, `BOSSn.SPT`, `BONUS_n.SPT`, `ICONE_n.SPT` (quest item icons, world + inventory),
`ILE_n.TXT` (dialogue), `INV_0n.RON`, `CARTE0n.RON`/`CARTE1n.RON` (map, revealed/unrevealed),
`DIAL_n.RON` (dialogue background), `CIEL_n.RON` (sky), `GO_n.RON` (game over).

## Top-level flow — **verified** (`main` = `0x12050`)

```
main:
  init CD + sound (0x11ef0)
MENU:                                  ; 0x1205e
  set video mode, clear palette
  main_menu()                          ; 0x25b00 -> sets exit / new_game / resume flags
  if exit:   shutdown; exit(0)
  if new_game (0x50d5e2 == 1):  new_game_reset(0)   ; 0x292d0(0): lives 3, energy 99, money 0 ...
  if not resume (0x50d5e0 == 0):                    ; i.e. new game or next island
      load ILE_n.UNI once (flag 0x40014)
      island_reset[island]()           ; table 0x415b8: 0x215b0/0x21640/0x216d0/0x21760/0x217f0
                                       ;   = monster type tables + MONSTERn.TRJ + spawn all monsters
      new_game_reset(1)                ; 0x292d0(1): per-island transient state only
      island_start[island]()           ; 0x17680/0x19030/0x1aa80/0x1c760/0x15ce0
                                       ;   = start position, ILE0n.TRJ, random money, quest items
ENTER:                                 ; 0x121db  (also the "continue" re-entry point)
  island_assets(island)                ; 0x11fc0 -> 0x174c0 etc.: sprite banks, meshes
  load ANIMAPS.RON, CIEL_n.RON, ILE_n.COL; fade in
  play sample 3 (mer11, sea loop) on channel 0; open DEMOILEn.DAT; CD track 5 if music on
  while !ESC && lives >= 0 && next_island == 0:
      poll mouse; while left button held: paused (keyboard handler removed)
      frame_common()                   ; 0x2a260
      island_frame[island]()           ; 0x187e0 / 0x1a6d0 / 0x1c450 / 0x1e080 / 0x173d0
      shop_check()                     ; 0x2a450 (runs the shop if zone == 0x12)
  if next_island == 2: next_island = 6          ; !!! see "Island 1 ends the game" below
  close demo file
  if next_island == 6:                          ; ending
      CD track 7; fade; play PART_1.MOV (15 loops arg), PART_2.MOV (1); 
      next_island = 0; lives = -1; goto MENU
  if next_island in 3..5 (2 is unreachable):
      show TRANS(n-1)-(n).RON (fade speed 0x96); island = next_island; resume flag = 0
      -> falls into the "not resume" branch above (next island, state kept: money, weapons, lives)
  else (ESC pressed or lives < 0):
      game_over_or_pause()             ; 0x255f0
      if it returned "continue" (0x50d5e2 == 1):
          energy = 99; lives = 3; continues -= 1; goto ENTER   ; quest progress is kept
      goto MENU
```

`frame_common` (`0x2a260`), in order — **verified**:
1. hurt timer `player.hurt` (`0x517eec`): if > 0 decrement; when it is exactly 19 on entry play
   sample 2 (duck quack).
2. jetpack fuel: if flying, `fuel -= 5`; if `fuel < 1` while flying: stop flying, `fuel = 0`.
3. `tick_loop()` (`0x28d50`): read input, then run the per-tick simulation
   (`demo_step`, clear sprite queue, clear mesh queue, `player_move` `0x10860`,
   `player_shots` `0x103b0`) `ticks_elapsed + 1` times. See `player.md` "Timing".
4. `player_draw` (`0x11400`): shadow + duck sprite, death sequence, respawn.
5. `shot_explosions` (`0x10290`).
6. `monsters_update(player.dead)` (`0x11be0` → `0x21af0`): AI, monster shots, damage, queue
   sprites.
7. CD track restart check every 10th frame (`0x11f70`).
8. view keys: F1/F2 view size, Backspace weapon select, Enter jetpack (`0x11960`).

`island_frame` (island 1 = `0x187e0`; others have the same shape) — **verified**:
1. sea-sound volume from the number of zone-1 (water) cells in the 30×30 cells around the
   camera (`0x188e0`): `vol = min(count,400)*255/400`.
2. island quest script (`0x178d0` …).
3. inventory screen if the inventory key was pressed (`0x185a0`).
4. animated terrain tiles (`0x11b40`).
5. pickups: animate, collide, queue sprites; death trigger when energy < 1 (`0x18160`).
6. quest items: queue sprites, compute `touch` (`0x184a0`).
7. map screen if Tab was pressed (`0x280f0`).
8. queue the island's fixed 3D meshes (`0x14050` calls with constants).
9. render + HUD + present (`0x148a0`; HUD = `0x246b0`).

Note the consequence of this order: the quest script of frame N sees the `touch` value
computed at the end of frame N-1. **verified**

## Island 1 ends the game in this build — code **verified**, "preview cut" **hypothesis**

At `0x124ea`: `if (next_island == 2) next_island = 6;`. Island 1's script sets
`next_island = 2` (`0x178d0`, step 11 on the build site), so completing Fisher Island
plays `PART_1.MOV`/`PART_2.MOV` with CD track 7 and returns to the main menu with
`lives = -1`. The `TRANS1-2.RON` branch at `0x12601` is dead code. Islands 2–5 chain
normally (3, 4, 5 → transition picture → next island; island 5 sets 6 → ending).

Evidence about intent:

- `PART_1.MOV` (13 frames, viewed with ffmpeg) shows the duck with three freed ducks next
  to open cages on the lava island — it is the **real ending** of the game, set on island 5,
  not an "end of part one" card. `PART_2.MOV` is 72 frames, starting black.
- The same value 6 is what island 5's script requests.
- All five islands' code, data, transition pictures and the 1→2 branch are present.

So the two-instruction override is a cut applied on top of a complete game: either a
one-island preview/demo build or a late hack. Nothing in the executable distinguishes the
two (no "demo"/"shareware" strings, version text is only `Punky Duck v1.0` in
`HELPME.DOC`). **Recommendation for the port:** make it a switch; default to the evident
design (island 1 → `TRANS1-2.RON` → island 2), with "faithful to this binary" as an option.

How later islands are reached in the original binary:

1. **Cheat menu** (`0x27880`): type `THE TRUTH IS OUT THERE` in a menu, then NEW GAME lists
   the five islands (`save-and-settings.md` 4). Starts with a fresh character on that island.
2. **Save games**: the save stores `island` first and the full island state; loading enters
   that island directly. A save can only be *made* on islands 2–5 after reaching them via
   (1) or via the chain 2→3→4→5, which works normally.

## Death, game over, continue — **verified**

- Energy (`0x517f20`, 0..99) below 1 starts the death sequence (`player.md`). At its end:
  `lives -= 1`, energy 99, teleport to the island start position.
- `lives < 0` leaves the frame loop → `0x255f0`: shows `GO_n.RON`. With `continues == 0`
  it waits for Enter and returns to the menu. Otherwise it shows `CONTINUE 9` with YES/NO
  and a countdown of 599 menu iterations (digit = `counter / 60`); YES → continue as above;
  NO or timeout → menu.
- ESC during play also goes through `0x255f0`: since `lives >= 0` it only sets the
  "game in progress" flag (`0x50d5e3 = 1`) so the main menu shows the 6-item variant with
  RESUME GAME / SAVE GAME. **verified** (`0x25ed0`, menu tables `0x41714` / `0x41728`)
- Continues (`0x517f18`) start at 0 and are only gained from pickup type 3.

## CD audio tracks (argument of `cd_play` `0x1e740`) — **verified** call sites

| Track | When |
|---|---|
| 4 | main menu (`0x25b00`) |
| 5 | island gameplay (`0x12431`), restarted when it ends (`0x11f70`) and after respawn |
| 6 | death sequence start (`0x18160`) |
| 7 | ending (`0x1251d`) |

Track 3 and 8+ are never requested by `P.EXE` (the argument `3` at `0x1241d` is a sample
index, not a CD track). **verified**

## Sample indices (argument of `sfx(n, volume)` `0x2ef10`)

Index = position in the name table at `0x41a84` (**verified** table; index→sample mapping
is **verified** only through `samples_load_all` loading in table order, see `functions.md`).

| n | Sample | Used for (call site) |
|---|---|---|
| 0 | bonus11 | pickup of energy/life/continue/jetpack/compass/etc. (`0x29d20`), plan pickup island 2 |
| 1 | boom11 | explosion of weapon 2..4,6,7 shots (`0x10290`); boss hit while alive (`0x23be0`) |
| 2 | duck11 | hurt (hurt timer == 19; drowning) |
| 3 | mer11 | sea ambience loop, channel 0 (`0x1241d`), volume by nearby water |
| 4 | monste11 | monster notices the player (random alt. with 28) |
| 5 | pack11 | jetpack on (`0x11960`), shop jetpack purchase |
| 6 | zap11 | firing weapon 3, 4 |
| 8 | flopd-11 | wading footstep in zone 1 (`0x11400`) |
| 12 | yes11 | F1 / F2 view change |
| 13 | lance11 | firing weapon 0, 1, 7 |
| 14 | dollar | money pickup, shop purchase |
| 15 | gat11 | firing weapon 5 |
| 16 | impact11 | explosion of weapon 0/1 shots |
| 17, 18 | mdead1/2 | monster death (odd/even shot slot) |
| 19, 20 | mtouch1/2 | monster hit (odd/even shot slot) |
| 21 | punch11 | bumping into a solid decor object on foot |
| 22 | teleport | teleporter |
| 23 | weapon11 | weapon pickup/purchase/select |
| 24 | zop11 | firing weapon 2, 6 |
| 25 | pose | item removed from inventory (`0x286b0`); shop "can't buy" |
| 27 | good | quest step advanced; shop energy purchase |
| 28 | rire | monster notices the player (alt.) |

7, 9, 10, 11, 26 (flopg, pas-g, pas-d, move, take) have no call site in the game code
reviewed. **hypothesis** (not exhaustively searched in the menu code).
