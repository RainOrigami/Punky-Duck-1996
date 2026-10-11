# Settings, save games, demo recording, cheats

**verified** against the code at the cited addresses unless marked otherwise.

## 1. `GAME.STP` (32 bytes, 8 × i32) — load `0x258c0`, save `0x25a40`

Loaded when the main menu opens, rewritten every time the main menu closes. Layout and
ranges: `formats/stp.md`. What the gameplay code does with each field:

| # | Addr | Field | Effect |
|---|---|---|---|
| 0 | `0x41610` | sound effects on | copied to `0x51c2d8`; the mixer only runs when 1 |
| 1 | `0x41614` | CD music on | gates every `cd_play` call |
| 2 | `0x41618` | controller 0 keyboard / 1 joystick | `read_input`; falls back to keyboard if the joystick read fails |
| 3 | `0x4161c` | SFX volume | `<< 4` into the mixer |
| 4 | `0x41620` | CD volume | `<< 4` |
| 5 | `0x41624` | difficulty 0/1/2 | see below |
| 6 | `0x416e4` | language 0 FR, 1 EN, 2 ES, 3 DE, 4 PL | dialogue line choice; shop messages (0..3 only; Polish shows nothing there) |
| 7 | `0x4000c` | game speed −6..+6 | see below |

Shipped file: `1, 1, 0, 15, 5, 1, 1, 0`.

### Game speed

Only used in `tick_loop` (`0x28d50`, `player.md` 3.1): `ticks = elapsed_70Hz_ticks +
game_speed`, minimum 1 (when the sum is below 1 the game also waits one retrace). So it
adds or removes whole 70 Hz movement ticks **per rendered frame**:

- it changes how fast the duck moves and its shots fly/reload (tick-based things), and —
  because the turn step is `0.07/ticks` applied `ticks+1` times — slightly changes the
  turn rate;
- it does **not** change monsters, monster shots, timers, animation (frame-based things).
- Negative values only help on slow machines (several ticks per frame); at one tick per
  frame they do nothing except add a retrace wait. With −6 every frame that takes at most
  six ticks runs exactly two steps, which makes the original deterministic per frame
  (used by the reference comparison). Frames that follow a palette fade (island entry,
  respawn) still run more steps: observed 60–61 after the entry fade and 18 after a respawn.

### Difficulty

| Quantity | Formula (`d` = 0/1/2) | Where |
|---|---|---|
| monster detect radius | `(d*5 + 12)` cells → 3600 / 5100 / 6600 units | `0x24110`, respawn in `0x21af0` |
| monster attack radius (shooters) | `(d*3 + 6)` cells → 1800 / 2700 / 3600 | same |
| monster leash radius | `(d*10 + 20)` cells | same |
| monster HP | `(type + 2*island) * (d+1) + 2` | same |
| monster shots at once | `d + 1` | same |
| monster fire chance per frame | `(d*20 + 30) / 256` | same |
| monster / boss shot speed | `d*50 + 150` per frame | same |
| boss HP | `(d+1) * island * 20 + 150` | `0x21af0` |
| boss fire chance | `(d*50 + 100) / 256` | `0x21af0` |
| exit condition after the boss | none / fewer than half the monsters alive / fewer than 2 | island scripts |

Difficulty is stored in the save game and restored on load (it overrides the option).
Changing it in the options during a game affects only monsters created afterwards
(respawns, boss) and the exit condition.

## 2. Save games

Menu: with a game in progress the main menu has RESUME GAME and SAVE GAME (`0x25ed0`).
Eight slots; slot names (10 bytes each, default `FREE 1`..`FREE 8`) live in `SAVEGAME.REC`
(80 bytes, rewritten after each save). Saving asks for a name (`ENTER NEW NAME`, max 8
characters from `a-zA-Z0-9 `, letters upper-cased; empty → `ISLAND%02d`). Loading asks
`LOAD <name> ?` with NO / YES.

Slot file: `SAVEGAME.__n`, `n` = slot index `0..7` (`0x26050` load, `0x26100` save). Plain
`fwrite` of globals, no header, no checksum. Total 181 817 bytes:

| Offset | Size | Content |
|---|---|---|
| 0 | 32 | i32: `island`, f32 `angle`, `step`, `step_shown`, `busy`, `0x517f78`, `model`, `model2` |
| 32 | 80 | `inv[20]` |
| 112 | 32 | `px`, `py`, `pz`, `tx`, `0x517efc`, `tz`, `boss_warned`, `ninv` |
| 144 | 200 | shot `type[0..49]` |
| 344 | 4 | `weapon` |
| 348 | 40 | `ammo[10]` |
| 388 | 40 | `owned[10]` |
| 428 | 88 | `shooting, flying, power, power_shots, fuel, hurt, money, continues, lives, energy, swim_timer, next_island, touch, touch_slot, dead, start_x, 0x517ebc, start_z, dead_wait, bump, bonus_last, item_last` |
| 516 | 800 | `bonus_frames[100]` pairs |
| 1316 | 32000 | bonus list (1600 × 20) |
| 33316 | 720 | quest item table (30 × 24) |
| 34036 | 8 | `compass`, `boss_state` |
| 34044 | 480 | teleporter markers x[30 ints: 10 used], z; hint markers x[30: 20 used], z (4 × 0x78 bytes: `0x516750`, `0x50da38`, `0x50d948`, `0x50d9c0`) |
| 34524 | 65536 | `explored` map |
| 100060 | 48 | island flags `0x517e9c, e90, e74, e70, e94, e98, ea0, e8c, e80, e88, e84, e7c` |
| 100108 | 81480 | monster array (210 × 0x184), incl. stale pointers at +0x154/+0x158 |
| 181588 | 220 | monster sprite tables `0x414c0` (5 × 11 i32) |
| 181808 | 5 | monster class bytes `0x4159c..0x415a0` (4 classes + low byte of `boss_muzzle[0]`) |
| 181813 | 4 | difficulty |

Offsets computed from the order of `fread` calls in `0x293c0` + `0x26050` (**verified**
order; sizes are the literal arguments). **Confirmed with the original**: a save written in
DOSBox-X is exactly 181 817 bytes and decodes with this table to island 1, step −1,
`px/pz = start = (−19650, −10575)`, lives 3, `bonus_last` 1471, `item_last` 8, difficulty 1,
900 explored cells (= one 30×30 block), monsters at their TRJ positions. The two pointers per monster point into the TRJ buffer, which sat at
`0x6ba144` in that run (`0x4f6144` + load base `0x1c4000`): `rec = buf + 4 + 8*npaths + 4 +
32*i`, `path = buf + 4 + 8*rec.path`. LOAD GAME by script needs `SAVEGAME.REC` next to
the save (`reference.md`). Not saved: player shots (positions/state), monster
`alive_count`, `outfit_on` (`0x517e78`; island 5 loads `DUCK.SPT` again after a load and
re-swaps on the next frame because the flag is reset by the asset function), random seed,
`boss_respawns`.

After a successful load (`0x264f0`): `trj_load_monsters()` (to rebuild pointers into the
TRJ buffer), `0x24650` (re-derive `path`/`rec` pointers for every monster), set "resume"
flag → `main` skips the island reset/start functions and enters the island with the loaded
state. The UNI is loaded only if none is loaded yet (flag `0x40014`, cleared when an island
ends) — **hypothesis**: loading a save of a different island while one is in memory works
because leaving the frame loop clears the flag (`0x12509`).

## 3. Demo recording / playback (`DEMOILEn.DAT`)

- The file is the raw stream of the 16-byte input block `0x50d5c4` (see `player.md` 7), one
  block per simulation **tick** (`demo_step` `0x25070` is the first call in the tick loop).
- Playback happens when `attract` mode is on (`0x41628 == 0`): each tick reads 16 bytes
  over the input block; at end of file it sets the Esc byte. `0x41628` is initialised to 0
  and set to 1 by NEW GAME / LOAD (`0x27880`, `0x264f0`); in attract mode any real key
  press sets it to 1, and `game_over` returns straight to the menu.
- **hypothesis**: attract mode is entered when the main menu idles (the menu functions
  reset `tick_counter` and `0x2781a..0x27855` compare it; not decoded), island = last
  island. No `DEMOILEn.DAT` ships on the disc, so `fopen` fails and playback reads from a
  NULL stream — the shipped game probably never shows a demo (`open-questions.md` Q5).
- `menu_run` clears the record flag every time the menu opens (`0x25b18`), so recording
  can only be switched on from inside the game. `reference.md` ("Deterministic state
  comparison") patches that store to record every run.
- Recording: in cheat mode, holding scancodes 0x11 + 0x10 + 0x40 (AZERTY `Z` `A` + F6)
  reopens the file for writing and records every tick; 0x11 + 0x10 + 0x41 (F7) stops.
  Because the tick count per frame depends on machine speed, a demo only replays exactly on
  a deterministic tick schedule.

## 4. Cheats

### Level select
Typing `THE TRUTH IS OUT THERE` (22 characters, upper case as delivered by the menu key
reader) in any menu sets a flag (`0x416e0`); the screen flashes red. NEW GAME then opens a
`CHEAT MODE` menu: FISHER / OASIS / YETI / COLD / SMOKE ISLAND / CANCEL (`0x27880`).

### In-game cheats (`read_input`, `0x26ec0`)
Detected when exactly three keys are down. First enable: scancodes 0x23 + 0x2e + 0x14
(`H` `C` `T`). Then (letters as on an AZERTY keyboard, scancodes in brackets), each
latching a flag in `0x4191a` that is **re-applied every frame**:

| Keys | Scancodes | Effect each frame |
|---|---|---|
| `L` `I` `F` | 26, 17, 21 | `lives = 3` |
| `N` `R` `J` | 31, 13, 24 | `energy = 99` |
| `A` `M` `O` | 10, 27, 18 | `ammo[0..7] = 9999`, all weapons owned |
| `J` `E` `T` | 24, 12, 14 | `fuel = 5000` |
| `F` `R` `Q`(azerty) | 21, 13, 1e | `power = 2` |
| `B` `O` `U` | 30, 18, 16 | `compass = 1` |
| `F` `R` `C` | 21, 13, 2e | `money = 500000` |

A palette flash confirms each. Flags stay on until the program exits.

## 5. Menus (for completeness)

- Main: NEW GAME, LOAD GAME, [RESUME GAME, SAVE GAME,] OPTIONS, EXIT. Up/Down move, Enter
  selects, **Esc jumps to the last entry and selects it** (`0x25c90`) — in the main menu
  that is EXIT, whose confirmation (`EXIT GAME ?` → `HO NO!` / `YES, SORRY...`) again has
  the quitting answer last, so two Esc presses quit the program. Samples 11 (move) and 12
  (select). Confirmed in DOSBox: Esc in play shows the 6-entry menu; further Esc taps
  exited the program.
- OPTIONS (`0x26660`): difficulty, language, game speed (Left/Right or Enter), RESOLUTION
  OPTIONS (VGA 320X200 / SVGA 320X200 / SVGA 640X400, FAST MODE), SOUND OPTIONS (sound,
  music, volumes), CONTROLLER OPTIONS (keyboard/joystick, calibrate), RETURN.
