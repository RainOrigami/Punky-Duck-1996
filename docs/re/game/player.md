# The duck (player)

All claims **verified** in the disassembly at the cited address unless marked
**hypothesis**. Floating-point expressions were recovered by tracing the x87 stack, integer
conversions are Watcom `__CHP` (`0x305a4`) = **truncate toward zero**; written `trunc()` below.
Integer division is C division (truncating). Names are proposals; addresses are in
`globals.md`.

## 1. World and units

- World units: 1 terrain cell = 300 units (`cell = [0x126e8]*[0x126e4] = 15*20`, stored in
  `0x4f0ec0` by `0x13390`). The terrain is 256×256 cells; world coordinates run
  −38400 … +38399 on X and Z, origin in the middle. Y is height, up positive.
- **The player position is stored negated.** `player.px` (`0x517edc`) and `player.pz`
  (`0x517ee4`) hold `−worldX`, `−worldZ`. Every consumer negates them again
  (`zone_at(-px,-pz)`, `terrain_height(-rx,-rz)`, sprite queue). Monsters, shots, pickups
  and quest items use true world coordinates. In this document `wx = -px`, `wz = -pz`.
- Facing angle `player.angle` (`0x517fa0`, f32 radians). Forward in world space is
  `(+cos a, +sin a)`; in the stored (negated) coordinates forward is `(−cos a, −sin a)`.
  Kept in range by `monsters_update` only: `if (angle > 0) angle -= 2π` each frame
  (`0x21afb`; i.e. it drifts to and stays ≤ 0 — it is never wrapped at the negative end).
- `terrain_height(x, z)` (`0x13010`): bilinear sample of the i16 height map `H[256][256]`
  at `0x1f2668` (`H[xc][zc]`, row stride 0x200 bytes):

  ```
  x += 38400; z += 38400
  xc = x / 300; xf = x % 300; zc = z / 300; zf = z % 300
  a = H[xc][zc]   + (H[xc+1][zc]   - H[xc][zc])   * xf / 300
  b = H[xc][zc+1] + (H[xc+1][zc+1] - H[xc][zc+1]) * xf / 300
  return ((b - a) * zf / 300 + a) * 300
  ```
  No bounds check (reads past the edge for coordinates outside the map).
- `zone_at(x, z)` (`0x151c0`): `xc = clamp(x/300, -126, 126)`, same for z, returns the byte
  `Z[xc+128][zc+128]` of the zone map at `0x22cdc8` (256×256 bytes, `[x][z]`). The zone map
  and height map come from `ILE_n.UNI` (see the formats spec); scripts also patch the zone
  map at run time (`zone_fill`, `0x28ce0`, note its Z index is off by one:
  it writes `Z[x][z+1]` for `x in x0..x1`, `z in z0..z1`).

### Zone types (as the player code treats them)

| Zone | Effect on the duck | Monster flags (`0x415d4[zone]`) |
|---|---|---|
| 0 | free ground | 0 |
| 1 | shallow water: speed /5, wading animation, no strafing | 0x12 |
| 2 | speed /3 | 0x14 |
| 3 | no player effect | 0x0b |
| 4 | no player effect | 0x04 |
| 5 | deep water: drowning counter (see 3.3) unless flying or swim timer | 0 |
| 6 | damage: every tick `energy -= 5`, `hurt = 20` (also when flying) | 0x0b |
| 7 | quest trigger (build site islands 1, 2) | 0 |
| 8 | solid | 0x0b |
| 9 | speed /2 | 0x04 |
| 10 | solid; quest trigger (islands 2, 3, 4) | 0x0b |
| 11 | no player effect | 0x0b |
| 12 | slippery: position follows target with divisors `4S` (x) and `6S` (z) | 0 |
| 13 | solid on foot, passable flying | 0 |
| 14 | solid on foot, passable flying; quest trigger island 3 | 0x0b |
| 15 | quest trigger (islands 3, 4) | 0x0b |
| 16 | quest trigger (island 4 runway) | 0 |
| 17 | raised: duck height +15000 (set by island 1 script, raft) | 0x0b |
| 18 | solid; **shop** | 0x0b |
| 19 | quest trigger (island 4) | 0 |
| 20 | solid; quest trigger (island 2 build site) | 0 |
| 21–24 | solid; quest triggers (island 5 vessels) | 0 |

Monster flag bits: see `entities.md`. What each zone number looks like on the island is a
property of the UNI data, not the code.

## 2. State

All are separate globals (no struct in the original); types are i32 unless noted.
`S` marks fields written to save games.

| Name | Addr | S | Meaning |
|---|---|---|---|
| `px`, `pz` | `0x517edc`, `0x517ee4` | S | position (negated world X/Z) |
| `tx`, `tz` | `0x517f00`, `0x517f14` | S | movement target; position chases it |
| `py` | `0x517ed8` | S | height of the duck sprite (terrain height, +15000 flying / zone 17) |
| `rx`, `rz` | `0x517f9c`, `0x517f98` | | copy of `px`,`pz` made at the end of each tick; used for height, drawing, shots |
| `cam_x, cam_y, cam_z` | `0x517f8c/90/94` | | `rx`, `6000` (= cell*20), `rz` |
| `angle` | `0x517fa0` f32 | S | facing |
| `slide` (`S`) | `0x517f84` | | smoothing divisor, per island 8/8/15/40/8 |
| `speed` | `0x40018` | | constant 34 (never written) |
| `fly_speed` | `0x4001c` | | constant 60 (never written) |
| `zone` | `0x517f80` | | zone type under the duck / under the target (see 3.2) |
| `flying` | `0x517ee8` | S | 0/1 jetpack active |
| `fuel` | `0x517f24` | S | jetpack fuel 0..4999 |
| `hurt` | `0x517eec` | S | hurt timer, set to 20 on damage, −1 per frame |
| `energy` | `0x517f20` | S | 0..99 |
| `lives` | `0x517f0c` | S | 3 at start, max 5, game over below 0 |
| `continues` | `0x517f18` | S | 0 at start |
| `money` | `0x517ed4` | S | |
| `dead` | `0x517ec0` | S | death sequence counter (0 = alive) |
| `dead_wait` | `0x517ed0` | S | second phase of the death sequence |
| `start_x`, `start_z` | `0x517eb8`, `0x517eb4` | S | respawn position (negated coords) |
| `swim_timer` | `0x517f08` | S | ticks of deep-water immunity (pickup 5 gives 300) |
| `drown` | `0x517f60` | | deep-water tick counter |
| `walk` | `0x517f68` | | walk animation counter (init 1 000 000 000) |
| `strafe_ctr` | `0x517f54` | | strafe animation counter (init 1 000 000 000) |
| `last_dir` | `0x517f64` | | +1 after moving forward, −1 after moving backward |
| `move_dir` | `0x517f7c` | | this tick: +1 forward, −1 backward, 0 none |
| `strafe` | `0x517f74` | | this tick: 0, 1 = left, 2 = right |
| `moved` | `0x517f5c` | | this tick: position changed |
| `bump` | `0x517eb0` | S | set by decor collision this frame: 0 none, 2 flying into decor, 3 walking into decor (1 is handled but never set) |
| `shooting` | `0x517f1c` | S | 2 when a shot is fired, −1 per frame; selects firing sprites |
| `weapon` | `0x517ee0` | S | selected weapon 0..7 |
| `power` | `0x517f10` | S | weapon power level 0..2 |
| `power_shots` | `0x517ef8` | S | shots left at this power level |
| `ammo[10]` | `0x517d80` | S | per weapon |
| `owned[10]` | `0x517970` | S | per weapon 0/1 |
| `fire_ctr` | `0x419ac` | | ticks since last shot / weapon change |
| `compass` | `0x517ea4` | S | has compass (map arrow + HUD compass) |
| `ticks` | `0x517f70` | | tick count of this frame (see 3.1) |

Initial values, new game (`new_game_reset(0)`, `0x292d0`): `continues=0, money=0, power=0,
weapon=0, fuel=0, lives=3, energy=99, owned[]=0 except owned[0]=1, ammo[]=0 except
ammo[0]=10 000 000`. Every island entry (`new_game_reset(1)`) clears: `shooting, flying,
hurt, strafe, drown, swim_timer, dead, dead_wait, compass, boss_state` and the island
flags `0x517e70/74/90/94`, and sets `touch=-1, model=-1, model2=-1, next_island=0`.
Note: the compass is lost on every island change; money, weapons, ammo, fuel, lives are kept.

The island asset function (`0x174c0` and siblings) runs on **every entry into the frame
loop** — new island, RESUME GAME after Esc, CONTINUE, LOAD GAME — and sets
`walk = strafe_ctr = 1e9, slide, busy = 0, touch = -1, touch_slot = 0, hurt = 0, drown = 0,
next_island = 0, compass = 0`, re-copies `bonus_frames` and zeroes all 250 player shots.
So pressing Esc and resuming, or loading a save, **loses the compass** (**verified**
`0x174c0`; the save does store it).

The HUD's displayed energy / money (`0x415fc`, `0x415f8`) start at 0 at program start and
are not reset or saved.

## 3. Movement

### 3.1 Timing

- A timer interrupt at **70 Hz** (PIT divisor `0x4295`, handler `0x2f832`) increments
  `tick_counter` (`0x41b02`).
- The main loop is not frame-limited. Once per rendered frame `tick_loop` (`0x28d50`) does:

  ```
  read_input(player.dead == 0)          ; 0x26ec0
  ticks = tick_counter; tick_counter = 0
  if sound_disabled ([0x40004] == 0): ticks = game_speed     ; timer ISR is still counting,
                                                             ; this branch just ignores it
  ticks += game_speed                   ; GAME.STP[7], option "GAME SPEED", default 0
  if ticks < 1: wait_retrace(); ticks = 1
  for (i = 0; i <= ticks; i++):         ; NOTE: ticks + 1 iterations
      demo_step(); sprite_queue_count = 0; mesh_queue_count = 0
      player_move(); player_shots()
  ```
- Per **step** (inside the loop): demo record/playback of the input block, zone lookup and
  zone damage (zone 6, drowning counter), walking/turn/strafe, knock-back displacement,
  world wrap, `py`, the walk-animation counter, weapon cooldown, firing, player shot flight
  and their terrain test. Input is sampled once per frame, so every step of a frame sees
  the same keys.
- Per **rendered frame**: hurt timer, jetpack fuel, death sequence and respawn, sprite
  choice, shot explosions, all monster logic (AI, movement, their shots, hits on monsters,
  contact damage, respawn rolls), pickups and decor collision, quest scripts, HUD counters,
  animated tiles, CD loop check, view keys, jetpack/weapon-select keys.
- So **movement and shot flight are per 70 Hz tick** (with one extra tick per frame), while
  everything else (hurt timer, fuel, monsters, pickups, animation, death) is **per rendered
  frame**. A port that wants original feel at 70 fps should run 2 movement ticks per frame
  (`ticks = 1`); at 35 fps 3 ticks. **hypothesis**: on period hardware the game ran at
  roughly 15–35 fps (see `open-questions.md` Q3).
- Turning divides by `ticks` (see below), so the turn rate is per frame, not per tick:
  one frame turns by `0.07 * (ticks+1)/ticks` rad.

### 3.2 `player_move` (`0x10860`), one tick

Inputs `up, down, left, right, strafe_key` are the bytes `0x50d5c4..c8`.

```
prev_x = px; prev_z = pz
moved = 0; move_dir = 0; strafe = 0
ltx = tx; ltz = tz                               ; local copies of the target

zone = zone_at(-px, -pz)
if !flying && zone == 5 && swim_timer > 0: zone = 0; swim_timer -= 1
if zone == 5 && !flying: drown += 1
if zone == 6: energy -= 5; hurt = 20

c = cos(angle); s = sin(angle)                   ; f32 angle, x87 extended evaluation
hF = terrain_height(trunc(c*300 + (-rx)), trunc(s*300 + (-rz))) / 300   ; 1 cell ahead
hC = terrain_height(-rx, -rz) / 300
                                                 ; (a third sample 1 cell behind is computed
                                                 ;  into 0x1c6ed4 and never used)
if !flying:
    slope = speed * (hF - hC) / 60               ; speed = 34
    v = speed - slope                            ; slower uphill, faster downhill
    if down: v = speed + slope
    if slide >= 15 && v < 0 && !up && !down:     ; steep slope on icy islands: slide back
        ltx = trunc(ltx - c*v); ltz = trunc(ltz - v*s)
    if zone == 9: v = v / 2
    if zone == 1: v = v / 5
    if zone == 2: v = v / 3
else:
    v = fly_speed                                ; 60

if drown > 10:
    drown = 0; hurt = 20
    if dead_wait == 0 && dead == 0: sfx(2, 255)
    energy -= 10

if 14 <= hurt <= 20:                             ; knock-back phase of a hit
    k = v / 4
    if last_dir == 1: px = trunc(c*k + px); pz = trunc(s*k + pz)      ; 0x10ba5
    else:             px = trunc(px - c*k); pz = trunc(pz - s*k)      ; 0x10bd9
    tx = px; tz = pz                             ; (symmetric; an earlier note claiming Z was
                                                 ;  always subtracted was a misreading)
else:
    if !strafe_key:
        if left:  angle -= 0.07 / ticks          ; f64 constant at 0x42288, stored back as f32
        if right: angle += 0.07 / ticks
    if hurt == 0:
        back = strafe_key ? 0.5 : 0.4
        if down: ltx = trunc(c*v*back + ltx); ltz = trunc(v*s*back + ltz); move_dir = -1
        if up:   ltx = trunc(ltx - c*v);      ltz = trunc(ltz - v*s);      move_dir = +1
        if strafe_key && zone != 1:
            angle += 1.57075                     ; temporarily, f32 round trip each time
            c2 = cos(angle); s2 = sin(angle)
            if left:  ltx = trunc(v*0.5*c2 + ltx); ltz = trunc(s2*(v*0.8) + ltz); strafe = 1
            if right: ltx = trunc(ltx - v*0.5*c2); ltz = trunc(ltz - s2*(v*0.8)); strafe = 2
            angle += -1.57075
    zone = zone_at(-ltx, -ltz)                   ; NOTE: zone of the *target* from here on

    if bump == 3:                                ; walked into solid decor: pushed a full step
        if last_dir == 1: px = trunc(c*v + px); pz = trunc(s*v + pz)   ; 0x10f01
        else:             px = trunc(px - c*v); pz = trunc(pz - s*v)   ; 0x10f35
        tx = px; tz = pz
    else if bump == 2:                           ; flying into decor: creep
        k = v / 16
        if up:   px = trunc(px - c*k); pz = trunc(pz - k*s)
        if down: px = trunc(c*k + px); pz = trunc(k*s + pz)
        tx = px; tz = pz
    else if bump == 1 && !flying:                ; same with k = v / 8 (bump is never 1)
        ...
        tx = px; tz = pz
    else if (zone == 14 && !flying) || zone in {8, 20, 18, 21, 22, 23, 24, 10}
            || (zone == 13 && !flying):
        tx = px; tz = pz                         ; blocked: target snaps back to the duck
    else:
        tx = ltx; tz = ltz                       ; accepted
        if !flying:
            if move_dir == +1: walk += 1; last_dir = +1
            if move_dir == -1: walk -= 1; last_dir = -1
        else:
            walk += 1
            if move_dir != 0: last_dir = move_dir

py = terrain_height(-rx, -rz)                    ; rx/rz = position at the end of the previous tick
if flying:     py += 15000
if zone == 17: py += 15000                       ; both can apply

if zone == 12 && !flying: px += (tx - px) / (slide*4); pz += (tz - pz) / (slide*6)
else:                     px += (tx - px) / slide;     pz += (tz - pz) / slide
if px != prev_x || pz != prev_z: moved = 1

if px >  42000: px -= 84000; tx = px             ; world wrap, well outside the 76800 map
if px < -42000: px += 84000; tx = px
if pz >  42000: pz -= 84000; tz = pz
if pz < -42000: pz += 84000; tz = pz

rx = px; rz = pz
cam_x = rx; cam_y = 6000; cam_z = rz
if zone <= 1: boss_warned ([0x517ef4]) = 0
```

`cos`/`sin` are re-evaluated from the stored f32 angle in every block (so movement of a
step uses the angle *after* that step's turn; the height samples use the angle before it).

**Verified against the original** (DOSBox-X, `reference.md` "Deterministic state
comparison"): replaying recorded per-step input through the port gives byte-identical save
games for walking, turning, firing, drowning with knock-back, death, respawn.

Consequences worth keeping in a port:

- Motion is "target + exponential chase": each tick the target moves by `v` and the duck
  covers `1/S` of the remaining distance. Steady-state speed is `v` units/tick (34 walking
  on flat ground = 0.113 cell/tick ≈ 7.9 cells/s at 70 ticks/s; 60 flying), with a lag
  of `S` ticks; on island 4 (`S = 40`) the duck visibly slides.
- Because the divisions truncate toward zero the duck stops `< S` units short of the target.
- Collision with the island is purely the **zone map lookup at the target point**; height
  differences never block (cliffs only slow down through `slope`). `ILE_n.COL` is a colour
  table and not involved (**hypothesis** from its loader, see formats spec).
- `zone` as seen by the scripts and the shop is the zone under the *target* after the first
  tick with no hurt; with `14 <= hurt <= 20` it is the zone under the duck.
- The script "eject" loops (`items-and-scripts.md`) and the shop exit move the duck by
  `px = trunc(cos(angle)*28 + px)`, `pz = trunc(28*sin(angle) + pz)` (28 = `[0x126e0]`)
  until `zone_at(-px,-pz)` is no longer the solid trigger zone, i.e. straight backwards.
  They do not update `tx/tz`.

### 3.3 Jetpack (`0x11960`, `0x2a260`)

- Enter (edge-triggered) toggles `flying` when `fuel > 0`; switching on plays sample 5.
- Per frame while flying: `fuel -= 5`; when `fuel < 1`: `flying = 0, fuel = 0`.
  A full tank (5000) lasts 1000 frames. Fuel is capped to 4999 by the HUD (`0x246b0`).
- Flying: speed 60 regardless of slope/zone, height +15000, zones 13/14 passable, deep
  water and decor damage type 0x1b harmless; zone 6 still hurts.

### 3.4 Decor collision (`pickups_update`, island 1 = `0x18160`)

Pickups and decor are the same list (`entities.md`). When the duck is within 120 units on
both axes of an entry (`|(-b.x) - px| < 0x78` and same for z) **and `hurt == 0`**:

- `apply_pickup(type, index)` (`0x29d20`) — collectibles, see `entities.md`.
- solid decor kind: `bump = flying ? 2 : 3`, and on foot `sfx(21)`.
- harmful decor kind: `energy -= 10; hurt = 20` (some only on foot).

Decor kinds differ **per island** (each island has its own copy of the function with its
own list; extracted from all five):

| Island | Fn | Solid (bump; sample 21 on foot) | Harmful (−10 energy, hurt 20) | Harmful on foot only |
|---|---|---|---|---|
| 1 | `0x18160` | 29, 30, 31, 44, 45 | 24, 26, 28, 34 | 27 |
| 2 | `0x1a200` | 26, 27, 28, 36–44 | 25, 31, 32 | 24 |
| 3 | `0x1be60` | 26–42 | — | — |
| 4 | `0x1dbb0` | 24–33 | 36, 37, 38, 39 | — |
| 5 | `0x16f90` | 26, 27, 28, 31, 32, 33, 38 | 24, 25, 29, 30, 34, 35, 36, 37 | — |

All other kinds ≥ 24 are pass-through decoration on that island. Kinds 0..23 are the
collectibles on every island (`apply_pickup` is shared).

`bump` is reset to 0 at the end of `player_draw` each frame, so it is valid for the ticks
of the following frame only.

## 4. Damage, death, respawn

Sources of damage (all subtract from `energy`; none check invulnerability except through
`hurt`, noted per source):

| Source | Amount | Sets `hurt = 20` | Blocked while `hurt != 0` | Where |
|---|---|---|---|---|
| zone 6 | 5 per tick | yes | no | `0x108fb` |
| drowning (11th tick in zone 5) | 10 | yes | no | `0x10b2e` |
| harmful decor | 10 | yes | yes | `0x18160` |
| monster body contact (within 125 units, height diff < 8000) | 1 per frame, plus money loss `10*(rand()%50+1)` if `money` is larger than that | yes | no | `0x23ec0` |
| monster shot (within ±100 x/z, y within −100…+10000 of `py`) | `monster.type` (1..5; boss = 5) | **no** | no; skipped when `dead != 0` | `0x23260` |

`hurt` timeline (per frame, `0x2a260`): 20 on the hit; frame where it is 19 on entry plays
the quack; while 14..20 the duck is knocked back, cannot turn or move, and shows the hit
animation; 13..1 it cannot move forward/back/strafe (it can turn) and blinks (drawn only on
even values); 0 normal. `hurt` is also set to 20 without damage by teleporters.

Death (`0x18160` tail, `0x11400`):

```
pickups_update (per frame):  if energy < 1 && dead == 0:
                                 dead_wait = 0; dead = 35; CD track 6 (if music)
player_draw (per frame), while dead != 0:
    energy = 0
    draw DUCK frame (25 - dead)/5 + (zone == 1 ? 0x33 : 0x28)
    if dead > 2 && dead_wait == 0: dead -= 1
    if dead_wait > 0: dead_wait -= 1
    if dead == 2: dead_wait = 50; dead = 1
    if dead_wait == 10:
        dead_wait = 0; dead = 0; energy = 99; hurt = 0
        px = tx = start_x; pz = tz = start_z
        lives -= 1; request fade-in ([0x40058] = 1); CD track 5 (if music)
```

So: 33 frames of falling animation (frames 0x26 → 0x2c, in water 0x31 → 0x37), 40 frames
lying, then respawn at the island start with full energy. While `dead != 0` the gameplay
keys are not read (`read_input(0)` zeroes the block and fills only F1/F2/Esc), and monsters do not fire. `lives < 0` ends the frame
loop (`overview.md`). Quest state, inventory, monsters and pickups are untouched by death.

Quirk (`0x246b0`): the HUD's displayed energy chases the real value by ±1 three times per
frame; if the *displayed* value goes below 0 (`energy` negative for a frame while the
display is at 0..2) the HUD itself does `lives -= 1; displayed = 99`. This can cost a
second life on a big hit at very low energy. **Confirmed in the original**: in two recorded
DOSBox runs (`out/simcmp/b4`, `t2`) the duck died once and the save shows `lives` 3 → 2 at
the start of the death sequence and → 1 after the respawn.

## 5. Weapons (`player_shots` `0x103b0`, `shot_explosions` `0x10290`)

Eight weapons 0..7 (`weapon` is masked with 7 by the HUD; arrays have 10 slots). Weapon 0
is owned from the start with 10 000 000 rounds. Selection: Backspace cycles to the next
weapon with `owned && ammo > 0` (wraps to 0); the HUD also auto-advances when the current
weapon has 0 ammo.

Shot pool: 250 slots, structure-of-arrays, ring allocation with `next_shot`
(`0x517ea8`):

| Array | Addr | Meaning |
|---|---|---|
| `state[250]` | `0x50dab0` | 0 free; 1..99 flying (ticks left); 200 → 196 exploding; 201..209 delayed explosion; −1 see quirk |
| `x, y, z` | `0x50e280`, `0x50de98`, `0x50e668` | world position |
| `vx, vz` | `0x517588`, `0x516db8` | per-tick velocity (a third array `0x5171a0` is written 0 and unused) |
| `type` | `0x517998` | weapon index (saved in save games; 200 bytes = first 50 only) |

Fire (per tick):

```
fire_ctr += 1
if last_weapon ([0x40028]) != weapon: last_weapon = weapon; fire_ctr = 0
if fire && fire_ctr > cooldown[weapon][power] && ammo[weapon] > 0:
    sfx: weapon 0,1,7 -> 13; 2,6 -> 24; 3,4 -> 6; 5 -> 15
    if power_shots > 0: power_shots -= 1; if power_shots == 0: power = 0
    i = next_shot
    x[i] = -rx; z[i] = -rz; y[i] = py + 9000; type[i] = weapon; ammo[weapon] -= 1
    a = autoaim_found ([0x50c7a0]) ? autoaim_angle ([0x50c79c]) : angle
    vx[i] = trunc((rand()%20 + 60) * cos(a))        ; two separate rand() calls
    vz[i] = trunc((rand()%20 + 60) * sin(a))
    state[i] = (weapon == 7) ? 49 : 99
    next_shot = (next_shot + 1) % 250; fire_ctr = 0; shooting = 2
```

`cooldown` (`0x419b0`, i32[weapon][power], in ticks): weapons 0–6 `{25, 15, 8}`, weapon 7
`{200, 150, 100}`. `rand()` is the Watcom LCG `seed = seed*0x41c64e6d + 0x3039;
return (seed >> 16) & 0x7fff` (`0x30570`).

Flight (per tick, every slot with `0 < state < 100`; the loop runs to index 250 inclusive,
one past the pool):

```
ground = terrain_height(x, z)                 ; at the position before the move
state -= 1
x += vx; z += vz
if type != 5: queue BOOM.SPT frame (state % 2) + 0x23 + type*2 at (x, y, z), scale 4.0
if y < ground || (type == 7 && state == 20):
    state = 200
    if type == 7:                             ; cluster: 39 delayed explosions around it
        for k in 1..39: j = (i + k) % 250
            type[j_previous] = type[i]        ; see below
            state[j] = rand()%10 + 200
            x[j] = x[i] + rand()%2400 - 1200; z[j] = z[i] + rand()%2400 - 1200
```

Cluster quirk (`0x107a7`, **verified** in assembly): the type is stored through register EDI
*before* EDI is set to the new `j`, so slots `i+1 .. i+38` get type 7, slot `i+39` keeps its
old type, and the first store of a call goes through whatever EDI the caller had (a stray
write the port does not model). The flight loop's extra slot 250 aliases `y[0]` as its state
and is never in the flying range in practice.

Shots fly horizontally at `py + 9000` and die when the terrain rises above them or the
timer runs out (silently, no explosion, for types other than 7). `autoaim_*` is set by
`monsters_update` when a live monster lies within about ±0.39 rad of the facing direction
(`0x236a0`; exact comparison in `entities.md`).

Explosion (per frame, every slot with `state > 100`):

```
if state <= 200:
    if state == 200: sfx(type < 2 ? 16 : 1) unless type == 5
    if type == 7: queue BOOM frame (230 - state) at (x, -300, z), scale 6.0
    else:         queue BOOM frame (200 - state) + type*5 at (x, y, z), scale 6.0
    if state < 197: state = 0
state -= 1
```

Quirk: after the last explosion frame the slot is left at **−1**, not 0. Monster hit
tests treat any non-zero state as a live shot (`entities.md`), so a spent explosion keeps
damaging monsters that walk within 250 units of it until the slot is reused or consumed by
a hit.

Damage to monsters is `(weapon + 2) / 2` using the **currently selected** weapon, not the
shot's type: 1,1,2,2,3,3,4,4 for weapons 0..7.

Weapon power (`power` 1 or 2, `power_shots = 400`) only shortens the cooldown.

## 6. Sprite selection (`player_draw`, `0x11400`, per frame)

Bank `DUCK.SPT` (58 frames; island 5 swaps it for `DUCK_1.SPT` once the commando outfit is
complete). Drawn at world `(-rx, py, -rz)` with scale parameter −1.0. First match wins:

| Condition | Frame |
|---|---|
| `dead != 0` | `(25 - dead)/5 + 0x28`, or `+ 0x33` in zone 1 |
| `14 <= hurt <= 20` | `(20 - hurt)/2 + (flying ? 0x1c : 0x18)` |
| `hurt` odd | not drawn |
| on foot and `strafe != 0` | `(strafe_ctr/6) % 4 + (strafe == 1 ? 0x20 : 0x24)`; then `strafe_ctr += 1` |
| on foot and `zone == 1` | `(walk/6) % 6 + 0x2d`; plays sample 8 once each time that index becomes 1 |
| on foot and `!moved` | `0x38`, or `0x39` while `shooting != 0` |
| otherwise | `(walk/6) % 6 + (1 - flying) * (shooting ? 0x12 : 0x0c)` |

So: flying 0..5, walking 12..17, walking+firing 18..23, hit 24..27 (flying 28..31), strafe
left 32..35, right 36..39, death 38..44 (overlaps strafe-right frames; as coded), wading
45..50, death in water 49..55, idle 56, idle firing 57. Frames 6..11 are not referenced.
`moved`, `strafe`, `zone` are the values left by the last tick of the frame.

Before the duck, the function queues DUCK frame `0x2d` at
`(trunc(cos(angle)*3000 + (-rx)), 0, trunc(sin(angle)*3000 + (-rz)))` with scale −999.0,
i.e. 10 cells ahead of the duck at height 0. Its purpose (aim marker? camera anchor?) is
unknown — **open**, see `open-questions.md` Q6.

After drawing: `if shooting > 0: shooting -= 1; bump = 0`.

## 7. Input (`read_input`, `0x26ec0`)

Keyboard state array `0x51d1f0[scancode]` (extended keys at `0x80 + code`). Gameplay input
block: 16 bytes at `0x50d5c4` (this exact block is what demo files store per tick).

| Byte | Meaning | Key | Trigger |
|---|---|---|---|
| `c4` | up / forward | Up | level |
| `c5` | down / backward | Down | level |
| `c6` | left | Left | level |
| `c7` | right | Right | level |
| `c8` | strafe modifier | either Ctrl | level |
| `c9` | jetpack toggle | Enter | edge |
| `ca` | next weapon | Backspace | edge |
| `cb` | view size cycle | F1 | edge |
| `cc` | interlace toggle (640×400 view only) | F2 | edge |
| `cd` | fire / confirm | Space | level |
| `ce` | map | Tab | edge |
| `cf` | inventory | PgUp, Home, Ins, Del, End or PgDn | edge |
| `d0` | menu / quit to menu | Esc | edge |
| `d1` | quest picture 1 | `1` | edge |
| `d2` | quest picture 2 | `2` | edge |

Bytes `c4..ca, cd..cf, d1, d2` are only filled when `read_input` is called with argument 1.
Joystick mode (`GAME.STP[2] == 1`): direction 1..8 = N, NE, E, SE, S, SW, W, NW from
`0x41948`; button A (`0x4194c`) = fire; button B (`0x41950`) = strafe modifier while a
direction is held, jetpack toggle (edge) when centred; with the stick centred the keyboard
keys are OR-ed in. `P` pauses (busy-waits for release, then for the next press). F12 writes
`SCRSHTnn.RAW`. Holding the left mouse button pauses (main loop).

## 8. HUD (`0x246b0`), numbers a port needs

- Energy shown as two digits plus a face icon `(99 - shown)/10`; lives as up to 5 hearts
  (`lives` is clamped to 5 here); money as 7 digits, counting toward the real value in
  steps of 1000/100/10/1.
- Fuel bar length `fuel/50` pixels (colour by thirds: < 1250, < 2500, else).
- Weapon icon `weapon`, power icon when `power != 0`, 4-digit ammo when `weapon > 0`.
- Compass (if owned): heading `trunc(angle * 57.2957787)` degrees, needle sprite
  `((deg + 68) / 45) & 7`.
- Boss bar while the boss is alive: `boss.hp * 100 / (max + 1)` with
  `max = (difficulty+1) * island * 10 + 100`.
- Layout coordinates differ per view size (`[0x4002c]`: 2 = 640×400, else half-size); see
  the function for positions.
