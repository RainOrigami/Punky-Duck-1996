# Quest items, inventory, island scripts, shop

**verified** = read in the code at the address given; the island scripts were read in full
in decompiled form (island 1 also in assembly). Names as in `globals.md`.

## 1. Is progression data-driven?

No. Each island has one hand-written C function (`quest script`) that runs every frame and
contains every pickup rule, dialogue id and build step as `if` chains on a handful of
globals. The only data are: the dialogue text (`ILE_n.TXT`), the quest item placement
(constants in the island start function) and the zone numbers painted in the island's
attribute map. A port should implement the five scripts as code (or transcribe the tables
below into data — they are complete).

## 2. State used by the scripts

| Name | Addr | Meaning |
|---|---|---|
| `step` | `0x517f2c` | quest progress counter, −1 at island start (saved) |
| `step_shown` | `0x517f50` | last `step` for which the "good" jingle (sample 27) was played |
| `touch` | `0x517f28` | id of the quest item the duck stands on, −1 if none; computed at the end of the previous frame |
| `touch_slot` | `0x517eac` | its slot in the item table |
| `busy` | `0x517f40` | 1 after a refusal message; cleared when `touch == -1`. Stops the message repeating while the duck stays on the item |
| `zone` | `0x517f80` | see `player.md` |
| `inv[20]`, `ninv` | `0x517e20`, `0x517ef0` | inventory: list of item ids in pickup order |
| `model`, `model2` | `0x517f30`, `0x517f4c` | which mesh of `OBJETS_n.3D` to draw at the build site(s), −1 = none |
| `boss_state` | `0x517ec8` | 0 none, −1 spawn requested, 1 alive, 2 dead |
| `boss_warned` | `0x517ef4` | "must finish the mission" message shown; reset whenever `zone <= 1` |
| `alive_count` | `0x50c798` | live monsters this frame |
| `nmonsters` | `0x414a8` | monsters in `MONSTERn.TRJ` |
| `next_island` | `0x517f04` | set to leave the island |
| island flags | `0x517e70..0x517ea0` | per-island booleans (saved), used as noted |

### Quest item table

30 slots × 24 bytes at `0x516ae8`: `{i32 id (-1 free), i32 icon, i32 radius?, i32 x, i32 y,
i32 z}`; highest used slot in `item_last` (`0x517ecc`). `item_add(id, x, y, z, icon, p6)`
(`0x27ec0`) fills the first free slot (`icon = arg5`, third field = `arg6`).

`items_update` (per island, island 1 = `0x184a0`), every frame, for each live slot:

```
dx = (-item.x) - px;  dz = (-item.z) - pz           ; as floats
if dx < 4500:                                       ; signed, X only
    queue ICONE_n.SPT frame item.id at (x, y, z), scale 4.0
    if dx*dx + dz*dz < 24000 && item.icon < 100:    ; radius ≈ 155 units
        touch = item.id;  touch_slot = slot
if nothing touched: touch = -1; busy = 0
```

- The world sprite frame is the item **id**; the `icon` field is only compared with 100
  (items dropped by `inv_remove(id, 1)` get `icon = 100` and can never be picked up again).
- The sixth argument (`p6`: 0, 200, 300, 500, 600, 1000) is stored and never read by any
  code reviewed — **verified** for the scripts, item, map and save code; meaning unknown
  (**hypothesis**: a score or price that was dropped).
- `y` is −300 (`0xfffffed4`) for almost every item: 300 units below the terrain reference
  used by the sprite queue (the queue stores `y / 300`).
- "Removing" an item from the world = `item[touch_slot].id = -1`.

Helpers: `inv_has(id)` `0x28d20`; `inv_remove(id, drop)` `0x286b0` (plays sample 25, closes
the gap; with `drop != 0` re-adds the item at the duck's position with icon 100);
`item_rename(old, new)` `0x28b70` (changes the id of matching slots — note it scans slots
`0 .. bonus_last-1`, using the wrong counter; see "Scan overflow" below); `hint(id)` `0x27e50` and `mark(id)` `0x27de0`
add the item's position to the map marker lists (20 "hint" markers colour 0xfd, 10
"teleporter" markers colour 0xfe; see `sections.md`); `say(n)` `0x28e00` shows dialogue
line `n`; `picture(name)` `0x28750`; `zone_fill(x0, z0, x1, z1, v)` `0x28ce0`.

### Scan overflow of `item_rename` / `item_remove_all` — **verified** (`0x28b70`, `0x28670`)

Both loop to the *bonus* counter (about 1471) over 24-byte item slots, i.e. 35 KB starting
at `0x516ae8`, far past the 30-slot table, and replace every i32 on that 24-byte grid that
equals the id. Gameplay globals on the grid (`0x516ae8 + 24 k`): `vz[6j]` of the player
shots (k = 30 + j), `vx[4 + 6j]` (k = 114 + j), `owned[0]`, `owned[6]`, `ammo[4]` (199),
animated-tile counter 0 (200), `inv[0]`, `inv[6]`, `inv[12]`, `inv[18]` (205–208), island-5
vessel flag `0x517e80`, `bump`, `boss_state`, `weapon`, `power_shots` (214), `power`,
**`touch`** (216, `0x517f28`), `busy`, `ticks`, `island`, `angle`. Practical effects:

- arming a teleporter (`item_rename(a, a+1)`) also changes `touch` itself from `a` to `a+1`
  (harmless: `hurt` is 20 by then);
- `power_shots`, `ammo[4]`, a shot velocity component or the tile counter equal to the pad
  id get bumped by one; on island 4 `item_remove_all(10)` sets `ammo[4]` / `power_shots`
  to −1 if they are exactly 10.

### Dialogue (`say`, `0x28e00`; text lookup `0x25140`)

Fades out, draws `DIAL_n.RON` (island 5: `DIAL_51.RON` once the outfit is on, flag
`0x517e78`), finds the block of `ILE_n.TXT` whose id line matches `n`, takes the line for
the current language (`0x416e4`: 0 FR, 1 EN, 2 ES, 3 DE, 4 PL) and prints its four quoted
strings centred at x = 320, y = 100 / 130 / 160 / 190 with font `COOLFNT1`. Waits for any
input press and release, fades back. The game is frozen meanwhile (`tick_counter` is reset
afterwards). See `formats/txt.md` for the file syntax.

### Inventory screen (island 1 = `0x185a0`)

On the inventory key: `INV_0n.RON` background, each `inv[i]` drawn from `ICONE_n.SPT` in a
4-column grid, cell 100×100: `x = (i % 4)*100 + 70`, `y = (i / 4)*100 + 90` (island 2: 120),
16 frames of 80×80 at `(30 + col*100, 50 + row*100)`. Frame index = `inv[i] + 8` on island 1,
`inv[i]` on islands 2–5 (`0x19fd0`, `0x1bc30`, `0x1d980`, `0x16d60`). Closes on fire.

## 3. Common script skeleton

Every script has the same shape:

```
1. teleporters (always, even while busy)
2. draw the build-site mesh(es) if model != -1
3. "1" / "2" keys: show the plan picture again if the plan is owned
4. if busy < 1:  item rules, zone rules, boss and exit rules, step jingle
```

Item rule pattern — `PICK(id, lo..hi, ok_line, refuse_line)`:

```
if touch == id:
    if lo <= step <= hi:  say(ok_line); remove item from world; inv[ninv++] = id; step += 1
    else:                 say(refuse_line); hint(id); busy = 1
```

(The order of `say` and the bookkeeping varies and does not matter.)

All pads of an island are armed first, then all active pads are tested (island 1: 15, 17,
then 16, 18).

Teleporter pattern — `TELEPORT(a, dest)`: item `a` is the inactive pad, `a+1` the active one:

```
if touch == a:    item_rename(a, a+1); hurt = 20; mark(a+1)       ; first contact arms it
if touch == a+1 && hurt == 0:  px = tx = dest.x; pz = tz = dest.z; sfx(22)
```

So stepping on a pad arms it (the 20-frame hurt stun doubles as a delay) and teleports the
duck when the stun ends if it is still standing there; afterwards the pad works whenever
`hurt == 0`. Destinations are in stored (negated) coordinates.

Boss / exit pattern — `FINALE(s, site_zone, warn, easy, medium, hard)`:

```
at step s (after the last build): boss_state = -1 (spawn)            ; trigger differs per island
if step == s+1 && zone == site_zone && !boss_warned && boss_state == 1: say(warn); boss_warned = 1
if step == s+1 && boss_state == 2:  say(difficulty == 0 ? easy : 1 ? medium : hard); step += 1
if step == s+2:
    difficulty 0:                                   step += 1
    difficulty 1 and alive_count < nmonsters / 2:   step += 1; say(easy)
    difficulty 2 and alive_count < 2:               step += 1; say(easy)
if step == s+3 && zone == site_zone: next_island = island + 1       ; island 5: 6, see below
```

`easy` = "Ok! I can leave now", `medium` = "…rid the island from at least half of its
monsters", `hard` = "…all of its monsters". Since monsters stop respawning once
`boss_state != 0`, the medium/hard conditions are reachable.

Finally `if step_shown != step: sfx(27); step_shown = step`.

## 4. Island 1 — raft (`0x178d0`; items placed by `0x17680`)

Items (`id @ (x, z)`, `p6`): 0 plan @ (18562, 1687) 200; 1 nails @ (19724, 5864) 300;
2 liana @ (−13500, −25500) 300; 3 plank @ (3000, −600) 500; 4 plank @ (−28930, −6818) 500;
5 trunk @ (21000, 23400) 600; 6 sail @ (−13500, 25500, y −600) 1000; teleporter pads (`p6` 0) 15 @
(31200, 15000) and 17 @ (−21300, −12900). Extra pickup: compass (type 7) at (19950, 3300).
Fixed meshes drawn every frame: see `0x187e0`.

```
TELEPORT(15, (20700, 22200));  TELEPORT(17, (12225, -17100))
if model != -1: mesh(slot 6, model, at (29400, 0, -17550), yaw -1.5708)
if busy < 1:
    key "1" and step >= 0: picture("RADEAU.RON")
    if zone == 7 && !site_seen (0x517e9c): say(6); site_seen = 1
    if touch == 0: picture("RADEAU.RON"); inv += 0; step += 1; remove            ; step -1 -> 0
    PICK(2, 0..1, 0→say(0), 4)        ; liana  (refusal 4 = "find the plan first")
    PICK(1, 0..1, 1, 4)               ; nails
    touch 4 / touch 3 (planks, lines 2 / 3):
        step 0..1 → say(7), hint, busy          ("something else to do before")
        step 2..3 → if site_seen: PICK ok  else say(5), hint, busy   ("find a place to build")
        else      → say(4), hint, busy
    if step == 4 && zone == 7: step = 5; inv_remove(3); inv_remove(4); say(10); model = 2
    if step > 4: zone_fill(0xe1, 0x45, 0xe3, 0x46, 17)      ; raft footprint becomes raised zone 17
    if step == 6 && zone == 7: step = 7; inv_remove(5); say(11); model = 1
    touch 5 (trunk):  step 2..4 → say(8) refuse ("too heavy"); step < 2 → say(7) refuse;
                      step == 5 → say(9), take, step = 6; (step > 5: nothing)
    touch 6 (sail):   step 2..5 → say(8) refuse; step < 2 → say(7) refuse;
                      step == 7 → say(12), take, step = 8
    if step == 8 && zone == 7: step = 9; inv_remove(6); say(13); boss_state = -1; model = 0
    FINALE(8, 7, warn 14, 15, 16, 17)  → next_island = 2
```

Because the boss is created on the frame after step 9 while the duck still stands on the
site, the "must finish the mission" line 14 follows line 13 immediately.

(`inv_remove(.., 0)` everywhere on this island.) Steps: −1 start, 0 plan, 1–2 liana+nails
(any order), 3–4 planks (any order, site must have been visited), 5 hull built, 6 trunk
carried, 7 mast up, 8 sail carried, 9 sail up + boss, 10 boss dead, 11 may leave.

## 5. Island 2 — ULM (`0x19300`; items by `0x19030`)

Items (all `p6` 200): 0 plan @ (−19500, −14400); 1 hatchet @ (−21637, 11737); 2 @ (24543,
1415); 4 @ (15590, −25209); 6 @ (19743, 27150); 5 @ (−30862, −12637); 7 @ (6356, 18356);
8 @ (−27290, −2034); 9 @ (−6787, 24075); 10 @ (−15900, −26775); 11 @ (−4846, 468);
pads 13 @ (−581, −30543), 15 @ (−33881, 29062), 17 @ (19368, −28171).

```
TELEPORT(13, (30281, -22912));  TELEPORT(15, (-14737, 16790));  TELEPORT(17, (-6093, -24787))
if model != -1: mesh(slot 0, model, at (-5862, 0, 5163), yaw -1.5708)
if busy < 1:
    if zone == 7 && !flag90 (0x517e90): add decor kind 44 (solid on island 2) at (-5923, 0, 4088); flag90 = 1; say(8)
    key "1" and step >= 0: picture("ULM.RON")
    if touch == 0: sfx(0); picture("ULM.RON"); inv += 0; step += 1; remove
    touch 1 (hatchet): step == 0 → say(0), take, step 1;  else say(1) refuse
    PICK(2, 1..3, 2, 3);  PICK(4, 1..3, 4, 5)
    if flag70 (0x517e70): zone_fill(0x0d, 0xda, 0x11, 0xdd, 0)        ; stolen-from spot opens
    if zone == 10 && !flag70:  step 1..3 → say(6); flag70 = 1; step += 1; inv += 3
                               else say(7), hint(touch), busy
    if zone == 7 && step == 4: say(9); inv_remove(4); inv_remove(3); inv_remove(2); step = 5; model = 0
    if step > 4: zone_fill(0x6b, 0x8d, 0x70, 0x91, 20); eject duck from zone 20 (player.md 3.2)
    touch 5: step 5..7 → say(10) take;  step == 4 → say(11) refuse;  else say(12) refuse
    PICK(6, 5..7, 13, 14);  PICK(7, 5..7, 15, 16)
    if zone == 20 && step == 8:  say(17); inv_remove(7); inv_remove(6); step = 9;  model = 1
    PICK(8, 9, 18, 19);    if zone == 20 && step == 10: say(20); inv_remove(8);  step = 11; model = 2
    touch 9: step == 11 → say(21), take as inventory id 12, step 12; else say(22) refuse
                           if zone == 20 && step == 12: say(23); inv_remove(12); step = 13; model = 3
    PICK(10, 13, 24, 25);  if zone == 20 && step == 14: say(26); inv_remove(10); step = 15; model = 4
    PICK(11, 15, 27, 28);  if zone == 20 && step == 16: say(29); inv_remove(11); step = 17; model = 5
    if zone == 0 && step == 17: say(30); step = 18; boss_state = -1
    FINALE(17, 20, warn 31, 32, 33, 34)  → next_island = 3
```

After a successful pick on this island the script also overwrites `touch` with a non-(−1)
register value (`0x19637` etc.), which only prevents later rules of the same frame from
seeing the old id.

## 6. Island 3 — shelter, radar, jetski (`0x1ae00`; items by `0x1aa80`)

Items (`p6` 200): 5 @ (1162, −29925); 3 @ (6112, −3637); 8 @ (13537, −27975); 0 @ (18675,
26100); 4 @ (27900, −12975); 9 @ (−5718, −18900); 10 @ (−5700, 15225); 1 @ (8850, 12975);
2 @ (24900, 8400); 11 @ (−23550, 900); 7 @ (−10518, 21562); 12 radar plan @ (150, −25500);
13 jetski plan @ (5250, −13200); pads 14 @ (−10509, −9553), 16 @ (23390, 23371), 18 @
(−20671, −928), 20 @ (2250, 4781), 22 @ (−4218, 27628). Extra decor kind 45 (pass-through on island 3) at
(−2315, −155, 150).

```
TELEPORT(14, (-18525, -29175)); TELEPORT(16, (27046, -17709)); TELEPORT(18, (-2981, 8409))
TELEPORT(20, (-25753, -5896))
pad 22: if touch == 22: item_rename(22, 21); hurt = 20; mark(23)   ; as coded: becomes a second pad 21,
        if touch == 23 && hurt == 0: teleport (-26081, 17596)      ; so the id-23 branch never fires
if model  != -1: mesh(slot 0, model,  at (12900, 0, -16800))
if model2 != -1: mesh(slot 1, model2, at (13050, 0, -15900))
key "1" and step >= 0: picture("RADAR.RON");  key "2" and inv_has(13): picture("JETSKI.RON")
if busy < 1:
    touch 12: picture("RADAR.RON"); take; step += 1
    touch 13: picture("JETSKI.RON"); if step == 6: step = 7; take
    PICK(5, 0, 0, 1);  PICK(3, 1, 2, 3)
    if zone == 15 && step == 2: say(4); inv_remove(5); step = 3; model = 4
    if step > 2: zone_fill(0xaa, 0x47, 0xac, 0x4b, 14); eject duck from zone 14
    touch 8: step == 2 → say(13) refuse; step 3..4 → say(5) take; else say(6) refuse
    touch 0: step == 2 → say(13) refuse; step 3..4 → say(7) take; else say(8) refuse
    if step == 5 && zone == 14: step = 6; add decor kind 43 (pass-through on island 3) at (13050, -300, -16200);
                                inv_remove(0); inv_remove(8); say(18)
    if step == 6 && inv_has(13): step = 7
    PICK(4, 7..9, 11, 12);  PICK(9, 7..9, 14, 15)          ; 9 = marmot trap
    if zone == 10 && step < 10:                           ; the marmot
        if !inv_has(9): text_on_picture(16, "MISS.RON")
        else: inv += 6; step += 1; text_on_picture(17, "CATCH.RON"); inv_remove(9, drop = 1)
    if zone == 14 && step == 10: step = 11; add decor kind 44 (pass-through) at the same place;
                                 inv_remove(6); inv_remove(4); say(19)
    if step == 11 && zone == 0: say(20); step = 12
    PICK(10, 12, 21, 22);  if step == 13 && zone == 14: step = 14; say(35); inv_remove(10); model2 = 0
    PICK(1, 14, 23, 24);   if step == 15 && zone == 14: step = 16; say(36); inv_remove(1);  model2 = 1
    PICK(2, 16, 25, 26);   if step == 17 && zone == 14: step = 18; say(30); inv_remove(2);  model2 = 2
    PICK(11, 18, 27, 28);  if step == 19 && zone == 14: step = 20; say(29); inv_remove(11); model2 = 3
    PICK(7, 20, 31, 32)
    if step == 21: say(37); step = 22; boss_state = -1
    FINALE(21, 14, warn 38, 39, 40, 41)  → next_island = 4
```

`text_on_picture(line, pic)` = `0x290e0`: like `say` but on the given picture.
The marmot zone 10 fires every frame the duck's target is on it while `step < 10`, with no
`busy` guard other than the outer one — standing there without the trap repeats the MISS
screen; with the trap it advances `step` by one each time (the trap is dropped as an
un-pickable icon), so in practice once.

## 7. Island 4 — tower, runway, glider (`0x1cae0`; items by `0x1c760`)

Items (`p6` 0, icon field 8): 8 @ (−8100, 21187); 9 @ (−24487, −3525); 6 @ (−25800, −16200);
10 @ (22125, 31425); 4 @ (−3900, −15525); 11 @ (−1800, −13000); 5 @ (28143, −21318); 3 @
(24675, 25125); 1 @ (24975, −12375); 2 @ (−7425, −4275); 7 @ (1453, −19621); 0 @ (−20475,
5325); 12 @ (−7500, 10500); 13 @ (−24000, 12300); 14 biplane plan @ (23700, 13200); 15 tower
plan @ (9093, −3693); pads (`p6` 200) 16 @ (28462, 9862), 18 @ (−20175, 18862), 20 @ (15862,
−12375), 22 @ (22931, −5268), 24 @ (−20981, −15393).

```
TELEPORT(16, (14475, -29025)); TELEPORT(18, (5250, -25425)); TELEPORT(20, (21412, -20437))
TELEPORT(22, (-25368, -5756));  pad 24: item_rename(24, 23); mark(25); id-25 branch dead (as island 3)
        ; hurt is 20 for all five pads (**verified** 0x1caff..0x1cbad: the register holds 0x14;
        ;  for pad 20 the compiler reuses the register that holds touch == 20)
        ; the dead id-25 branch would teleport to (1687, 7012) (0x1ccde)
if model  != -1: mesh(slot 0, model,  at (5380, 0, -15600))
if model2 != -1: mesh(slot 1, model2, at (4180, terrain_height(4180, -18400), -18400))
key "1" and inv_has(15): picture("TOUR.RON");  key "2" and inv_has(14): picture("BIPLAN.RON")
if busy < 1:
    touch 15: picture("TOUR.RON"); take; step += 1          ; step -1 -> 0
    touch 14: picture("BIPLAN.RON"); take                   ; no step change
    PICK(8, 0..1, 0, 1);  PICK(9, 0..1, 2, 3);  PICK(6, 2, 4, 5)
    if zone == 15 && step == 3: model = 1; step = 4; say(6); inv_remove(6); inv_remove(9)
    if step > 3: zone_fill(0x91, 0x4a, 0x93, 0x4c, 10); eject duck from zone 10
    PICK(10, 4, 7, 8)
    if zone == 10 && step == 5: step = 6; inv_remove(10); item_remove_all(10);
                                item_add(10, 5380, -51000, -15600, icon 10, 0); say(9)
    if zone == 16 && step == 6: step = 7; say(10)
    PICK(4, 7, 11, 12);  PICK(11, 8, 13, 14);  PICK(5, 9, 15, 16)
    if zone == 16 && step == 10: inv_remove(4, drop); inv_remove(11); inv_remove(5, drop); step = 11; say(17)
    PICK(3, 11..12, 18, 19);  PICK(1, 11..12, 20, 21)
    PICK(2, 13..15, 22, 23);  PICK(7, 13..15, 24, 25);  PICK(0, 13..15, 26, 27)
    PICK(13, 16..17, 28, 29); PICK(12, 16..17, 30, 31)
    if zone == 0  && step == 18: say(32); step = 19
    if zone == 19 && step == 19: step = 20; say(33); model2 = 3; inv_remove(12, drop); inv_remove(13)
    if step == 20 && zone == 0: say(34); step = 21; boss_state = -1
    FINALE(20, 19, warn 35, 36, 37, 38); on exit: text_on_picture(39, "DEPART.RON"); next_island = 5
```

`0x28670(id)` = `item_remove_all(id)`: sets `id = -1` in every item slot with that id
(scans slots `0..bonus_last`). The item is then re-added far below the terrain (y −51000)
at the tower, i.e. it only serves as a map/marker anchor (**hypothesis** for the intent). Mesh changes between steps 11 and 19 (wings etc.) are not
in this function: none found — the glider appears only at step 20 (**verified** for this
function; other per-frame island-4 functions were not searched).

## 8. Island 5 — commando (`0x160d0`; items by `0x15ce0`)

Items (`p6` 0): outfit 0 @ (−7950, 2550), 1 @ (23175, 3000), 2 @ (28500, 19425); bomb parts,
four copies each: id 3 @ (−25650, 22200), (−10575, −31125), (13425, −24600), (30075, −16200);
id 4 @ (−12900, 14475), (−31800, −12075), (−5625, −2550), (18750, 10050); id 5 @ (−900,
−28725), (13350, −10275), (150, 16575), (−19800, −21375); 6 @ (−15150, 6600); 7 @ (−24975,
−9450); 9 plan @ (−8700, 26850); pads (`p6` 200) 10 @ (18281, −4125), 12 @ (15825, −3225),
14 @ (30075, 5362), 16 @ (−7875, 7050), 18 @ (15900, −21750), 20 @ (−15375, 2475).

```
TELEPORT(10, (-13537, 937)); TELEPORT(12, (-14025, -14775)); TELEPORT(14, (7800, -150))
TELEPORT(16, (10725, -13200)); TELEPORT(18, (13275, -7050)); TELEPORT(20, (-20400, -28500))
meshes (always): slot 0 = vessel_a state (0x517e80) at (-26550, 0, 8540); slot 1 = 0x517e88 at
   (18159, 0, -6759); slot 2 = 0x517e84 at (19640, 0, 16340); slot 3 = 0x517e7c at (-2428, 0, -8397)
   ; mesh index 0 = intact vessel, 1 = wreck
key "1" and step >= 0: picture("COMMANDO.RON")
if busy < 1:
    touch 9: picture("COMMANDO.RON"); take; step += 1
    PICK(0, 0..2, 0, 1);  PICK(1, 0..2, 2, 3);  PICK(2, 0..2, 4, 5)
    if step == 3: inv_remove(1); inv_remove(2); inv_remove(0); step = 4; say(6)
    if !outfit_on (0x517e78) && step > 2: reload duck bank from "DUCK_1.SPT"; outfit_on = 1
    touch 3 / 5 / 4 (bomb parts): step < 3 → say(7) refuse
        else if already inv_has(id): say(8) refuse (4: say(13))
        else take, say(9) / say(12) / say(14)                 ; no step change
    touch 6: step == 4 → take, step = 5, say(15); else say(16) refuse
    if touch != -1 && step > 4 && inv_has(4) && inv_has(5) && inv_has(3):
        inv_remove(4); inv_remove(5); inv_remove(3); inv += 8 (bomb); picture("BOMBE.RON")
    touch 7 (lighter): step < 3 → say(18) refuse; else take, step += 1, say(17)
    for (zone, flag) in (21, 0x517e80), (22, 0x517e88), (23, 0x517e84), (24, 0x517e7c):
        if zone == zone_n && flag == 0 && inv_has(8):
            flag = 1; inv_remove(8)
            picture_timed("EXPLOSE.RON", 16); picture_timed("EXPLOS01.RON", 16); picture("EXPLOS02.RON")
            step += 1
    if step == 9 && all four flags set: step = 10
    if step == 10: boss_state = -1; step = 11
    if step == 11 && boss_state == 2: say(21 / 22 / 23 by difficulty); step = 12
    if step == 12:
        difficulty 0: next_island = 6; step = 13
        difficulty 1 and alive_count < nmonsters/2: step = 13; say(21); next_island = 6
        difficulty 2 and alive_count < 2:           step = 13; say(21); next_island = 6
```

Notes: the bomb is assembled whenever the duck touches *any* quest item while holding all
three parts (the test is `touch != -1`). `step` reaches 9 only as 5 (after item 6) + lighter
(+1, any time after step 3) + … — the exact count needed is "item 6, lighter, and enough
explosions to reach 9"; with lighter taken before item 6 the equality `step == 4` for item 6
fails and item 6 can never be taken, but that is **not a soft lock**: bombs only need
`step > 4`, and four explosions then bring `step` from 5 to 9 with all four flags set, which
is exactly the `step == 9 && all four` rule (with both items the fourth explosion reaches 10
directly). Resolved by reading the rules; not observed in play. Lines 10, 11, 19, 20 of `ILE_5.TXT`
(19/20 = "before leaving…", "mission isn't finished") are never shown; there is no
return-to-site condition on this island. The island-5 boss must be killed three times
(`entities.md`).

## 9. Fixed scenery meshes

`mesh(slot, model, x, y, z, rx, ry, rz, 1)` (`0x14050`) queues mesh `model` of
`OBJETS_n.3D` when the duck is within 4500 units on both axes. Positions are f32 world
coordinates passed as constants from the island frame function, e.g. island 1 (`0x187e0`):
slot 0 model 4 @ (15881, 3225); 1 model 5 @ (18318, 2700); 2 model 4 @ (18318, 4481);
3 model 5 @ (20662, 2981); 5 model 6 @ (−11868, −25050); 7 model 7 @ (−31600, 26850).
Island 2 (`0x1a6d0`): slots 1–3 model 8 @ (−1500, 11700), (1200, 11400), (−1800, 14100);
4–5 model 9 @ (−600, 12900), (2700, 12000); 6 model 7 @ (−34030, 27600) only while
`flag70 == 0`; 7 model 6 @ (−24900, −21900); 8 model 10 @ (−29100, −7900).
Island 3 (`0x1c450`): 2 model 5 @ (1200, −28500); 3 model 5 @ (25930, 7950) yaw −1.5708;
4 model 6 @ (16100, 26250). Island 4 (`0x1e080`): 2 model 2 @ (−23000, 30440).
Island 5: only the four vessels (section 8). The 7th argument is a yaw in radians (−1.5708 where used).
(Earlier notes called the calls to `0x27ec0` "3D object placement": `0x27ec0` is
`item_add` — quest item sprites — and its last argument is the unused `p6` above.)

## 10. Shop (`shop_check`, `0x2a450`)

Runs when `zone == 18` after the island frame (the shop cell is solid, so this means the
duck's target entered it). Modal loop on `MARCHAND.RON` with a hand cursor (`HAND.SPT`)
moved by the arrow keys (2 px per iteration, x 8..599, y 8..441) or the mouse.

Hot zones (`0x2b3a0(id, x0, y0, x1, y1)`): 0 = (28,274)-(127,294) **buy**, 1 = (28,312)-
(127,331) **cancel**, 2 = (14,437)-(93,459) **exit**; products 3..10 = eight 70×70 cells at
`x0 = 17 + 72*i, y0 = 20`; 11..17 = seven cells at `y0 = 105`; 18..21 = four cells at
`x0 = 161 + 72*i, y0 = 192`.

Fire on a product selects it (`sel = zone_id - 3`, its picture — `BONUS_n.SPT` frame from
the table below — is shown at (320, 368)). Fire on **buy** applies the selection, **cancel**
restores the undo copies ("Cancel!!!"), **exit** leaves.

Undo copies (**verified**, `0x2a450`): `owned[]`, `ammo[]`, fuel, compass, money,
`power_shots`, energy, power are copied when the shop opens, but every purchase attempt
overwrites the copy of the field it touches with the value from just before that purchase
(money: just before the last successful purchase; `power_shots` is never refreshed). So
cancel, "not enough money" and "not useful" all restore only **the last purchase of each
kind** and refund only the most recent price. Samples: energy 27, fuel 5, compass 0,
everything else 23; then 14 on success or 25 on failure. Ammo packs are 200, 150, 100, 50,
50, 50, 10. Buying is also possible with nothing useful selected → "not useful". Money is shown
at (342, 452).

| sel | Product | Price | Condition, else "not useful" | Effect |
|---|---|---|---|---|
| 0 | energy | 15000 | `energy < 90` | `energy = 99` |
| 1 | jetpack fuel | 30000 | `fuel < 4500` | `fuel = 5000` |
| 2 | compass | 30000 | not owned | `compass = 1` |
| 3 | power level 1 | 20000 | `power == 0`, or `power == 1 && power_shots < 300` | `power = 1; power_shots = 400` |
| 4 | power level 2 | 40000 | `power < 2`, or `power_shots < 300` | `power = 2; power_shots = 400` |
| 5, 7, 9, 11, 13, 15, 17 | weapon 1..7 | 10000, 15000, 30000, 40000, 60000, 70000, 90000 | not owned | `owned[w] = 1` |
| 6, 8, 10, 12, 14, 16, 18 | ammo for weapon 1..7 | 1000, 1500, 3000, 4000, 6000, 7000, 9000 | always | `ammo[w] +=` 200, 150, 100, 50, 50, 50, 10 |

`if price > money`: undo, "Not enough money"; else `money -= price`, sample 14, "OK".
Product pictures (BONUS frames): 0, 34, 37, 38, 39, then 40..53 for sel 5..18 (table at
`0x27d40`, pairs `{frame, price}`).

On exit the screen fades back and the duck is ejected from zone 18 (`player.md` 3.2).
Quirk: `power level 2` bought while `power == 0` and then `power == 1` can charge twice in
one press because the three `if`s run in sequence (each adds 40000 to the price variable by
assignment, not addition, so the price stays 40000) — harmless.
