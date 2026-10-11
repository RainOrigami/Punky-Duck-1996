# Monsters, bosses, pickups

**verified** against the disassembly at the cited addresses unless marked **hypothesis**.
Conventions as in `player.md` (world units, `trunc()`, `rand()`). `diff` = difficulty
0/1/2 (`0x41624`), `island` = 1..5 (`0x517f88`). Player world position below is
`(wx, wz) = (-rx, -rz)`, height `py`.

## 1. Monster array

210 slots × 0x184 bytes at `0x4f8944` (`monster[i]`), slot 0 doubles as the boss.
The whole array (0x13e48 bytes) is written to save games verbatim, including the two
pointers at +0x154/+0x158 (re-derived after load by `0x24650`).

| Off | Type | Name | Meaning |
|---|---|---|---|
| 0x00 | i32 | `type` | 0 = free; 1..4 monster type; 5 boss |
| 0x04 | i32 | `hp` | |
| 0x08 | i32 | `move_kind` | idle movement: 0 none (dying), 1 none (boss), 2 patrol between two points, 3 circle, 4 random walk, 5 follow TRJ path. Only 0, 1, 5 are ever assigned |
| 0x0c | i32 | `ai` | 0 idle, 1 chase, 2 attack, 3 return, 4 dying |
| 0x10, 0x14 | i32 | `x`, `z` | world position |
| 0x18 | i32 | `y` | height above terrain (300) |
| 0x1c, 0x20 | i32 | `home_x`, `home_z` | position where the chase started |
| 0x28 | i32 | `detect` | squared radius / 1000 |
| 0x2c | i32 | `attack` | squared radius / 1000 |
| 0x30, 0x34 | i32 | `speed_x`, `speed_z` | 100, 100 |
| 0x38 | i32 | `dist2` | `trunc(squared distance to player / 1000)` |
| 0x3c | i32 | `leash` | squared radius / 1000 around `home` |
| 0x40, 0x44, 0x48 | i32 | `prev_x, prev_z, prev_y` | position at the start of this frame's update |
| 0x4c | i32 | `hit` | 1 on the frame a shot hit it (sprite flag `hit*8 + mirror`) |
| 0x50 | i32 | `frame` | sprite frame to draw |
| 0x54 | i32 | `mirror` | 1 = draw mirrored |
| 0x58 | i32 | `anim` | 0 walk, 1 attack, 2 stand, 3 death |
| 0x5c | i32 | `dir` | 0..7 facing octant relative to the camera |
| 0x60 | i32 | `anim_idx` | frame index inside the animation |
| 0x64 | i32 | `anim_tick` | |
| 0x68 | i32 | `anim_period` | frames per animation step (1 monsters, 2 boss and dying) |
| 0x78 | i32 | `max_shots` | 0..5 |
| 0x7c | i32 | `fire_chance` | out of 256 per frame while attacking |
| 0x80 | i32[5] | `shot_state` | 0 free; > 0 flying (age in frames); < 0 impact animation (starts at −100) |
| 0x94, 0xa8, 0xbc | i32[5] each | `shot_x`, `shot_z`, `shot_y` | |
| 0xd0, 0xe4, 0xf8 | i32[5] each | `shot_vx`, `shot_vz`, `shot_vy` | per frame |
| 0x10c | i32 | `shot_speed` | units per frame |
| 0x114 | i32[5] | `shot_anim` | |
| 0x128 | i32 | `spread` | aim scatter (units) |
| 0x12c | i32[5] | `shot_frame` | |
| 0x140 | i32[5] | `shot_mirror` | |
| 0x154 | ptr | `path` | → path header in the TRJ buffer |
| 0x158 | ptr | `rec` | → this monster's 32-byte TRJ record |
| 0x15c | i32 | `path_idx` | next path point (kind 5) |
| 0x160, 0x164 | i32 | `seg_x0`, `seg_z0` | current segment start |
| 0x168, 0x16c | i32 | `seg_x1`, `seg_z1` | current segment end |
| 0x170, 0x174 | i32 | `step_x`, `step_z` | per-frame step along the segment |
| 0x178 | i32 | `seg_len` | |
| 0x17c, 0x180 | f32 | `rot_cos`, `rot_sin` | of the record's angle |

For `move_kind` 2/3/4 the fields from 0x154 on are reused with other meanings
(`0x22c60`); since those kinds are never assigned they are omitted here.

## 2. Spawn data

`MONSTERn.TRJ` is read whole (max 0x2800 bytes) to `0x4f6144` by `0x24000`. Layout:
`formats/trj.md` (path table `{u32 first_point, u32 point_count}`, 32-byte records, point
block `{f32 x, f32 z}`). Record fields used: `path index` (+4), `x`, `z` (+12, +16),
`scale` (+20), `angle` (+24), `kind` (+28); `monster.type = kind + 1`.
`record_count > 209` is fatal.

Paths 0 and 1 are single points (0,0): **stationary** monsters. Paths 2+ have 3–6 points.
Monsters per path, island 1: 81 stationary, 66 with a path (of 147); island 2: 105 / 55;
island 3: 106 / 93; island 4: 105 / 94; island 5: 65 / 135.

`monsters_init` (`0x24110`, called from the island reset function): clear the array, then
for each record `i` → slot `i`:

```
type = rec.type0 + 1;  move_kind = 5;  ai = 0;  y = 300;  speed_x = speed_z = 100
detect = ((diff*5 + 12) * 300)^2 / 1000
attack = cls == 2 ? 90 : cls == 3 ? 360 : ((diff*3 + 6) * 300)^2 / 1000
leash  = ((diff*10 + 20) * 300)^2 / 1000
hp = (type + island*2) * (diff + 1) + 2
anim_period = 1
if cls == 2 or cls == 3:  shot_speed = max_shots = fire_chance = spread = 0     ; melee only
else: shot_speed = diff*50 + 150; max_shots = diff + 1; spread = 600; fire_chance = diff*20 + 30
path_idx = 1; rot_cos/sin from rec.angle
seg0 = trunc(P[0]);  seg1 = count < 2 ? seg0 : W(seg0, P[1])       ; W: see 3.6
seg0 += (rec.x, rec.z);  seg1 += (rec.x, rec.z)
seg_len = trunc(sqrt((seg1.x-seg0.x)^2 + (seg1.z-seg0.z)^2))
step = seg_len == 0 ? (0, 0) : (speed_x*(seg1.x-seg0.x)/seg_len, speed_z*(seg1.z-seg0.z)/seg_len)
x, z = seg0
```

`cls = class_of[type]` (`0x4159b[type]`, set per island by the reset function):

| Island | type 1 | type 2 | type 3 | type 4 |
|---|---|---|---|---|
| 1 | 0 | 1 | 0 | 0 |
| 2 | 0 | 1 | 0 | 1 |
| 3 | 0 | 2 | 0 | 1 |
| 4 | 0 | 0 | 2 | 1 |
| 5 | 2 | 1 | 3 | 0 |

Class = terrain constraint (`0x218b0`, uses `flags = 0x415d4[zone_at(x,z)]`, table in
`player.md`):

- 0 and 3 (walkers): flag 2 → position reverts to `prev`; flag 4 → position becomes the
  midpoint of `prev` and the new position (half speed).
- 1 (flyers): flag 1 → revert.
- 2 (swimmers): must stay where flag 0x10 is set, else revert.

### Sprite tables

Per island, five 11-int records are copied to `0x414c0` (types 1..5; type 5 = boss, bank
`BOSSn.SPT`, others `ENEMYn.SPT`): `{base, walk_off, attack_off, stand_off, death_off,
shot_off, walk_n, attack_n, stand_n, death_n, shot_n}` (`*_n` = frames per direction, for
death the total).

| Isl | Type | base | walk | attack | stand | death | shot | walk_n | attack_n | stand_n | death_n | shot_n |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 1 | 0 | 0 | 30 | 45 | 50 | 54 | 6 | 3 | 1 | 4 | 1 |
| 1 | 2 | 59 | 0 | 5 | 25 | 30 | 35 | 1 | 4 | 1 | 5 | 1 |
| 1 | 3 | 99 | 0 | 5 | 15 | 20 | 23 | 1 | 2 | 1 | 3 | 1 |
| 1 | boss | 0 | 0 | 10 | 15 | 25 | 32 | 2 | 1 | 2 | 7 | 1 |
| 2 | 1 | 0 | 0 | 5 | 15 | 20 | 23 | 1 | 2 | 1 | 3 | 1 |
| 2 | 2 | 28 | 0 | 5 | 10 | 15 | 18 | 1 | 1 | 1 | 3 | 1 |
| 2 | 3 | 51 | 0 | 20 | 30 | 50 | 57 | 4 | 2 | 4 | 7 | 1 |
| 2 | 4 | 113 | 0 | 10 | 15 | 25 | 32 | 2 | 1 | 2 | 7 | 1 |
| 2 | boss | 0 | 0 | 5 | 20 | 25 | 30 | 1 | 3 | 1 | 5 | 4 |
| 3 | 1 | 0 | 0 | 5 | 15 | 20 | 24 | 1 | 2 | 1 | 4 | 1 |
| 3 | 2 | 29 | 0 | 5 | 5 | 10 | 15 | 1 | 0 | 1 | 5 | 0 |
| 3 | 3 | 44 | 0 | 20 | 30 | 35 | 42 | 4 | 2 | 1 | 7 | 1 |
| 3 | 4 | 91 | 0 | 5 | 25 | 30 | 35 | 1 | 4 | 1 | 5 | 1 |
| 3 | boss | 0 | 0 | 20 | 25 | 45 | 50 | 4 | 1 | 4 | 5 | 1 |
| 4 | 1 | 0 | 0 | 5 | 10 | 15 | 18 | 1 | 1 | 1 | 3 | 1 |
| 4 | 2 | 23 | 0 | 20 | 25 | 30 | 34 | 4 | 1 | 1 | 4 | 4 |
| 4 | 3 | 77 | 0 | 25 | 25 | 30 | 37 | 5 | 0 | 1 | 7 | 0 |
| 4 | 4 | 114 | 0 | 20 | 25 | 45 | 50 | 4 | 1 | 4 | 5 | 1 |
| 4 | boss | 0 | 0 | 40 | 60 | 65 | 71 | 8 | 4 | 1 | 6 | 1 |
| 5 | 1 | 0 | 0 | 35 | 35 | 45 | 51 | 7 | 0 | 2 | 6 | 0 |
| 5 | 2 | 51 | 0 | 20 | 25 | 45 | 51 | 4 | 1 | 4 | 6 | 1 |
| 5 | 3 | 107 | 0 | 5 | 5 | 35 | 36 | 1 | 0 | 6 | 1 | 0 |
| 5 | 4 | 143 | 0 | 20 | 25 | 30 | 36 | 4 | 1 | 1 | 6 | 1 |
| 5 | boss | 0 | 0 | 0 | 0 | 20 | 27 | 4 | 4 | 4 | 7 | 1 |

(Island 1 type 4 is all zero / unused.) Sources: `0x4105c + 0x2c*k`, copied by `0x215b0`,
`0x21640`, `0x216d0`, `0x21760`, `0x217f0`.

Frame formula (`0x23860`): `dir5 = dir <= 4 ? dir : 8 - dir; mirror = dir > 4`

```
frame = base + 8 + off[anim] + dir5 * n[anim] + anim_idx        ; anim 0,1,2
frame = base + 8 + death_off + anim_idx                          ; anim 3
```

Each direction block has 5 facings (0 = toward… see `dir` below; 5..7 are mirrors of 3..1).
Frames 0..7 of each bank are the shared shot-impact animation (impact uses 1..7).

## 3. Per-frame update — `monsters_update(wx, wz, py, player_alive)` (`0x21af0`)

Called once per frame from `0x11be0`, which afterwards queues the sprites: monster at
`(x, -y, z)` scale 4.0 (boss: bank BOSS, scale 12.0, its shots 8.0), each active shot at its
position with `shot_frame`/`shot_mirror`.

```
if player.angle > 0: player.angle -= 2π
alive_count = 0; autoaim_found = 0; autoaim_best = π; spawned = 0

; --- boss creation / bookkeeping --------------------------------------------
if boss_state == -1:                       ; requested by the island script
    load "Boss<island>.spt" into the boss bank
    slot 0: type 5; hp = (diff+1)*island*20 + 150; move_kind 1; ai 0; x = z = 0; y = 300
            speed 100/100; attack = 4500; detect = leash = 0x168000
            anim_period 2; shot_speed = diff*50 + 150; max_shots 5
            fire_chance = diff*50 + 100; spread 300
    boss_state = 1
    if island == 5: boss_respawns ([0x50c60c]) = 3
if boss_state == 1:
    if island == 5 && slot0.type == 0 && --boss_respawns != 0:
        re-create slot 0 exactly as above (island 5 boss must be killed 3 times)
    if slot0.type == 0: boss_state = 2

; --- pass 1: activation, respawn -------------------------------------------
for i in 0..209:
    if monster[i].type == 0:
        if boss_state == 0 && spawned == 0 && (rand() & 0xfff) == 0 && i < nmonsters:
            re-create monster i exactly as in monsters_init; spawned += 1
    else:
        alive_count += 1                                  ; 0x50c798, read by the scripts
        if |x - wx| < 6000 && |z - wz| < 6000  or  (boss_state == 1 && i == 0):
            active_list.push(i)
        else:
            clear shot_state[0..4]                        ; far monsters lose their shots

; --- pass 2: the active ones ---------------------------------------------------
for i in active_list:
    prev = (x, z, y)
    d2  = ((wz - z)^2 + (wx - x)^2) / 1000.0              ; f32
    dist2 = trunc(d2)
    d2h = ((home_z - z)^2 + (home_x - x)^2) / 1000.0
    dist = sqrt(d2 * 1000)
    switch ai:
      0: if d2 < detect: home = (x, z); ai = 1
                         sfx((rand() & 1) ? 28 : 4, trunc(255 - d2*200/detect))
         else idle_move(i)                                ; 0x22c60
      1: if move_kind != 0:
             if d2h < leash:                              ; axis-wise pursuit, no normalisation
                 if x < wx: x += speed_x;  if wx < x: x -= speed_x
                 if z < wz: z += speed_z;  if wz < z: z -= speed_z
             if anim_idx == 0:
                 if d2 < attack: ai = 2
                 else if d2 >= detect: ai = 3
      2: if d2 >= attack: ai = 1
         else:
             k = first free shot slot
             if player_alive && k < max_shots && (rand() & 0xff) < fire_chance: fire(k)
      3: if d2 < attack: ai = 2
         else if d2 < detect: ai = 1
         else if (x, z) == home: ai = 0
         else if move_kind != 0: step each axis toward home by speed, snapping when closer than speed
      4: (dying, handled by animate)
    animate(i)                                            ; 0x23860
    face_and_autoaim(i)                                   ; 0x236a0
    hit_by_player_shots(i)                                ; 0x23be0
    update_shots(i)                                       ; 0x23260
    body_contact(i)                                       ; 0x23ec0
    zone_constraint(i)                                    ; 0x218b0 (not for the boss)
    wrap x, z into [-38400, 38100]: < -38400 → += 76800; > 38099 → -= 76800
    if move_kind == 5 && path.count == 1: (x, z, y) = prev   ; stationary monsters
```

Note the last line: a monster on a single-point path **never changes position** — its
chase steps are undone every frame; only animation, facing and shooting react (seen in the
DOSBox capture: the plant monster next to the island-1 start stays put for a minute while
shooting). Monsters on multi-point paths and the boss (`move_kind 1`) do move and chase.

### 3.1 Firing (`0x227xx`)

```
rx = rand() % (spread + 1) - spread/2;  rz = likewise
shot_x = x; shot_z = z
shot_y = terrain_height(x, z) + y + 4000  (+ boss_muzzle[island] if type == 5)
         ; boss_muzzle (i32 at 0x415a0 + 4*island) = {0, 0, 0, 8000, 30000, 50000}; entry 0 is
         ; not an island: its low byte is `class_of[5]` and is set to 1 at boss creation
         ; (that byte is the fifth "class byte" in save games)
dx = wx - (x + rx); dz = wz - (z + rz); dy = (py + 6500) - shot_y
if d2 == 0: n = (1, 1, 1) else n = (dx, dz, dy) / dist
shot_vx = trunc(shot_speed * n.x); shot_vz = trunc(shot_speed * n.z); shot_vy = trunc(shot_speed * n.y)
shot_anim = 0 (**verified**: `mov edx,ebx` at 0x22847, `xor edx,ebx` at 0x228a5); shot_state = 1
; n is (1,1,1) only when d2 is exactly +-0.0 (bit test at 0x22816)
```

### 3.2 Monster shots (`0x23260`, per frame)

```
for each slot k < max_shots with shot_state != 0:
  if shot_state < 0:                       ; impact animation
      shot_frame = shot_state + 101;  shot_state += 1
      if shot_frame > 7: shot_state = 0
  else:
      n = shot_speed / 30;  old = (shot_x, shot_z);  shot_state += 1
      repeat n times:
          shot_x += shot_vx / n;  shot_z += shot_vz / n;  shot_y = max(shot_y + shot_vy / n, 0)
          if outside ±38400 or shot_state > 49: shot_state = 0; break
          if zone flag 8 at (shot_x, shot_z):    shot_state = 0; break
          if shot_y < terrain_height(shot_x, shot_z):
              shot_state = -100
              if type != 5 && dist > 600: ai = 1
              break
          if wx-100 < shot_x < wx+100 && wz-100 < shot_z < wz+100 && py-100 < shot_y < py+10000:
              shot_state = -100
              if player.dead == 0: energy -= type          ; 1..4, boss 5; no hurt timer
              shot = (wx, wz, py + 6500);  break
      d = trunc((atan2f(shot_x - old.x, shot_z - old.z) - (player.angle - 0.3927)) / 0.7854) & 7
      shot_mirror = d >= 5;  if d >= 5: d = 8 - d
      shot_anim = (shot_anim + 1) % shot_n
      shot_frame = base + 8 + shot_off + d * shot_n + shot_anim
```

`atan2f(dx, dz)` here is the game's helper `0x15230(dx, dz)`: `atan(dz/dx)` with +π added
when `dx < 0` (both sign cases), and ±π/2 / π for `|dx| < 1e-?` (constant at `0x4246c`).

### 3.3 Being hit (`0x23be0`, per frame)

For every player shot slot `j` (0..249) with `state != 0`, if `ai != 4`,
`|shot.x - x| < 250`, `|shot.z - z| < 250` and
`|shot.y / 16| - (terrain_height(x,z) - y) < 8000` (practically always true):

```
hp -= (player.weapon + 2) / 2;  hit = 1;  shot.state[j] = 0
if hp < 1:
    sfx(j odd ? 17 : 18, 255 - dist2*200/detect)
    move_kind = 0; ai = 4; anim_period = 2
    if boss_state == 1: sfx(1, 255 - dist2*200/detect)
else:
    sfx(j odd ? 19 : 20, 255 - dist2*200/detect)
```

Dying (`0x23860`): `ai == 4` forces `anim = 3`; when `anim_idx` reaches `death_n` the slot
is freed (`type = 0`) and its shots cleared. **Monsters drop nothing** and give no money;
killing them matters only for the "rid the island" quest step (`alive_count`) and for
safety. Random respawn (1/4096 per free slot per frame, at most one per frame, only while
`boss_state == 0`) refills them.

### 3.4 Body contact (`0x23ec0`, per frame)

If `|wx - x| < 125 && |wz - z| < 125 && |py - (terrain_height(x,z) + y)| < 8000`:
`r = rand() % 50 + 1; if r*10 < money: money -= r*10; energy -= 1; hurt = 20`.

### 3.6 Idle movement along a TRJ path (`idle_move` kind 5, `0x22f53`) — **verified** as code

```
W(c, P):  x = trunc(c.x + ((P.z - c.z)*rot_sin + (P.x - c.x)*rot_cos) * rec.scale)
          z = trunc(c.z + ((P.z - c.z)*rot_cos - (P.x - c.x)*rot_sin) * rec.scale)

idle_move(m):                                   ; only called in ai state 0
    if |m.x - seg1.x| < step.x && |m.z - seg1.z| < step.z:     ; signed compare with the step
        (m.x, m.z) = seg1;  path_idx += 1;  seg0 = seg1
        if path_idx == path.count: path_idx = 0
        if path.count >= 2:
            seg1 = W(seg0, P[path.first + path_idx])           ; seg0 is a WORLD position here,
            seg_len = trunc(sqrt(...))                         ; P is path-local; rec.x/z not added
        if seg_len == 0: return                                ; keeps the old step
        step = (speed_x*(seg1.x-seg0.x)/seg_len, speed_z*(seg1.z-seg0.z)/seg_len)
        return
    m.x += step.x;  m.z += step.z
```

There is no other code path for these monsters: `move_kind` is only ever assigned 5
(`monsters_init`, respawn), 1 (boss) and 0 (dying); kinds 2–4 in `0x22c60` are dead code.

Consequences of the code as written (simulated with the shipped data; the path points are
tiny, |P| ≤ 116 units, scales 0.03–0.37):

1. The first leg is a few units long (e.g. island 1 record 137 at (17925, 11250): target
   (17936, 11261)).
2. On arrival the next target is `W(world position, P)`, i.e. roughly
   `position * (1 - scale)` rotated by `angle` — a point thousands of units away toward
   the map origin (record 137: (11320, 7315)).
3. The arrival test compares an absolute distance with the **signed** step, so a leg whose
   step has a zero or negative component is never completed: the monster keeps walking in
   a straight line at 100 units/frame, wrapping around the world, until its zone
   constraint pins it (walkers/flyers revert to the previous position at forbidden cells)
   or the player comes within `detect` (then it chases) or leaves the 6000-unit activity
   box (then it freezes).

So "patrolling" monsters do not circle their spawn point; they drift off in a straight
line whenever the player is between `detect` radius and the activity box.
**Confirmed in the original** (`out/simcmp/q7`: an edited save with the duck 5400 units from
monster 81, loaded and left alone for 14 s): monster 81 went from (19762, 22762) to its
first target (19764, 22764), took the predicted step (−65, −75) toward (18086, 20836) and
was at (16039, 22689) in attack state at the end; monster 131 drifted from (19800, 20625) to
(15830, 16711) with step (−71, −70). The port's state after the same inputs is
byte-identical. Note for `seg_len`: when the path has one point the branch at `0x2318c`
keeps the old `seg_len`.

### 3.5 Animation and facing — **verified** (`0x23860`, `0x236a0` read in assembly)

`monster_animate(i)`:

```
if anim != 3:
    firing = any k < max_shots with shot_state[k] == 1      ; a shot launched this frame
    if ai == 4:        anim = 3; anim_idx = 0
    else if firing:    anim = 1; anim_idx = 0
    else if anim != 1 or anim_idx >= attack_n:              ; attack plays through first
        want = (x, z) == (prev_x, prev_z) ? 2 : 0
        if anim != want: anim = want; anim_idx = 0
dir5 = dir > 4 ? 8 - dir : dir;  mirror = dir > 4
frame = base + 8 + {walk_off + dir5*walk_n | attack_off + dir5*attack_n |
                    stand_off + dir5*stand_n | death_off}[anim] + anim_idx
anim_tick += 1
if anim_tick == anim_period:
    anim_tick = 0; anim_idx += 1
    anim 0: anim_idx %= walk_n;  1: anim_idx %= attack_n + 1;  2: anim_idx %= stand_n
    anim 3: if anim_idx == death_n: type = 0; shot_state[0..4] = 0     ; slot freed
```

`monster_face(i, wx, wz, py, trunc(d2))`:

```
h = (x - prev_x, z - prev_z);  if h == (0, 0): h = (wx - x, wz - z)
bearing = atan2f(x - wx, z - wz);  heading = atan2f(h.x, h.z)
dir = trunc((heading - (angle - 0.3926990875)) / 0.785398175) & 7
u = |bearing - angle + 0.19634954375|
v = u - trunc(u / 6.2831854) * 2.0 * 3.1415927
if v < 0.3926990875 and (f32)v < autoaim_best:
    autoaim_found = 1; autoaim_angle = bearing; autoaim_best = v
```

`autoaim_best` is reset to pi and `autoaim_found` to 0 at the start of each
`monsters_update`; player shots fired during the following frame's steps use the result.
Monster shots use the same octant formula on their direction of travel.

Order of `rand()` calls in `monsters_update` (matters for reproducing the original): pass 1
calls it once for **every free slot of all 210** (including the slots above `nmonsters`)
while `boss_state == 0` and nothing has been respawned yet this frame; pass 2 calls it for
the notice sound, the fire roll (only when the player is alive and a shot slot is free),
then twice for the scatter; body contact calls it once. First boss creation does **not**
clear slot 0: whatever monster 0 was doing carries over into the boss (only the island-5
re-creation does a `memset`). Body contact and shot flight also run for dying monsters.

All of section 3 is **confirmed against the original**: recorded runs replayed through the
port give byte-identical monster arrays (`reference.md`, "Deterministic state comparison").

## 4. Pickups and decor ("bonus" list)

1600 slots × 20 bytes at `0x50ea50`: `{i32 type (-1 = gone), i32 anim, i32 x, i32 y, i32 z}`,
highest used index in `bonus_last` (`0x517ec4`). Whole array saved in save games.

Filled at island start:

1. `ILE0n.TRJ` (`0x27fd0`): same container as the monster file; each 32-byte record gives
   `bonus[i] = {rec.type0, 0, rec.x, -600, rec.z}`. More than 1599 records is fatal
   ("Erreur nombre de bonus"); a type outside 0..54 is fatal ("Erreur BONUS").
2. Random money (`0x28bc0`): while `bonus_last < 1470`: pick random cells
   `cx = rand()%256, cz = rand()%256` until the 3×3 zone neighbourhood around it is all 0;
   `bonus_set(bonus_last++, 2, (cx-128)*300, -3000, (cz-128)*300)`. (This consumes `rand()` —
   the layout depends on the seed state at island start; from a fresh program, seed 1, the
   first cell is (198, 126). **Confirmed**: the port reproduces all 456 island-1 cells of a
   save written by the original.)
3. Script-placed extras, same `bonus_set(bonus_last++, ..)` call (island 1 adds a type 7
   compass at (19950, −3000, 3300), island 3 decor kind 45 at (−2315, −155, 150)).

**Append bug** (`0x28cb5`, `0x17768`, **verified** in assembly and in the original's save):
`bonus_last` is the highest *used* index, but appending writes at `bonus_last` and then
increments. So the first random money **overwrites the last `ILE0n.TRJ` record** (on island 1
that is the TRJ's own compass at (28162, 27187), index 1014), and the list always ends with
one slot that was never written: after a fresh start it is all zero = an energy pill
(type 0) at world (0, 0, 0), index 1471 on island 1. Each later append overwrites the
current phantom and creates a new one from whatever the slot held.

Per frame (`pickups_update`, per-island copies `0x18160`, `0x1a200`, `0x1be60`, `0x1dbb0`,
`0x16f90`): for each live entry: animate, test pickup (see `player.md` 3.4: within 120
units on both axes and `hurt == 0`), queue its sprite from `BONUS_n.SPT` at scale 4.0 if
`|(-b.x) - px| < 4500` (the Z distance is not tested — the same X comparison is coded
twice).

Animation: `range = bonus_frames[type]` (`{first, last}` pairs at `0x5167c8`, copied from
`0x403b4`): if `first < last`: `anim = (anim + 1) % ((last - first) * 2);
frame = first + anim / 2`, else `frame = first`. Ranges: type 0 → 0..9, 1 → 10..13,
2 → 14..23, 3 → 24..33, type `t >= 4` → single frame `t + 30`.

### Pickup effects (`apply_pickup`, `0x29d20`); the entry is then removed (`type = -1`)

| Type | Effect | Sample |
|---|---|---|
| 0 | `energy = 99` | 0 |
| 1 | `lives += 1` (HUD clamps to 5) | 0 |
| 2 | `money += island*100 + 100` | 14 |
| 3 | `continues += 1` | 0 |
| 4 | `fuel = 5000` | 0 |
| 5 | `swim_timer = 300` | 0 |
| 6 | `money += 10000` | 0 |
| 7 | `compass = 1` | 0 |
| 8 | if `power < 2`: `power = 1, power_shots = 400` (otherwise not collected) | 23 |
| 9 | `power = 2, power_shots = 400` | 23 |
| 10, 12, 14, 16, 18, 20, 22 | `owned[1..7] = 1` | 23 |
| 11 / 13 / 15 / 17 / 19 / 21 / 23 | `ammo[1..7] +=` 200 / 150 / 150 / 150 / 100 / 100 / 8 | 23 |
| 24..54 | decor; solid / harmful / pass-through **per island**, table in `player.md` 3.4 — stays | 21 when bumping on foot |

What the kinds look like (frames of `BONUS_1.SPT`, viewed in `out/gfx/spt/BONUS_1.png`;
frame = kind + 30 for kind ≥ 4; the collectible kinds 0..23 are the same on all islands by
code, the decor art and its collision class differ per island; pictures below are island 1 only,
see `out/gfx/spt/BONUS_n.png` for the others):

| Kind | Frame(s) | Picture |
|---|---|---|
| 0 | 0–9 | red/white pill (energy), animated |
| 1 | 10–13 | "1up" |
| 2 | 14–23 | spinning "$" |
| 3 | 24–33 | spinning "C" (continue) |
| 4 | 34 | jetpack |
| 5 | 35 | purple flask (deep-water immunity) |
| 6 | 36 | money bag |
| 7 | 37 | compass |
| 8, 9 | 38, 39 | "x2", "x3" (weapon power) |
| 10 / 11 | 40 / 41 | pistol / its ammo |
| 12 / 13 | 42 / 43 | purple gun / purple ammo |
| 14 / 15 | 44 / 45 | ray gun / ammo |
| 16 / 17 | 46 / 47 | blunderbuss / red shells |
| 18 / 19 | 48 / 49 | machine gun / clip |
| 20 / 21 | 50 / 51 | red gun / fireball |
| 22 / 23 | 52 / 53 | bazooka / rocket |
| 24 | 54 | thorny red plant (harmful on island 1) |
| 25 | 55 | purple flower |
| 26 | 56 | big red toadstool (harmful) |
| 27 | 57 | small toadstool (harmful on foot) |
| 28 | 58 | purple carnivorous plant (harmful) |
| 29, 30, 31 | 59, 60, 61 | palm, broadleaf tree, cypress with fruit (solid) |
| 32, 33 | 62, 63 | small bushes |
| 34 | 64 | wide berry bush (harmful) |
| 35–38 | 65–68 | bamboo / reeds |
| 39–43 | 69–73 | orange mushrooms |
| 44, 45 | 74, 75 | big trees (solid) |
| 46 | 76 | hot-air balloon |
| 47–54 | — | accepted by the loader, no frame in `BONUS_1.SPT` (77 frames) |

Script-placed kinds 43/44/45 on islands 2–3 are therefore island-specific art (built
objects), not the island-1 pictures.

Type usage in `ILE01.TRJ` (1015 records): 0×5, 1×1, 4×3, 5×3, 6×2, 7×1, 8..10×1, 11×70,
12×1, 13×22, decor 24..45 the rest.

## 5. Quest items

Separate 30-slot table, see `items-and-scripts.md`.
