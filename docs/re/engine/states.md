# Top-level state machine (P.EXE `main`, `0x12050`)

**Verified** from the disassembly of `0x12050`–`0x126dd` unless marked. `DUCK.BAT` runs
`INTRO.EXE`, `P.EXE`, `THE_END.EXE` in sequence; P.EXE itself contains menu → islands.

## Globals

| Address | Meaning |
|---|---|
| `0x517f88` | current island 1..5 (also patched as a digit into `ILE_n.UNI`, `ILE_n.TXT`, `MonsterN.Trj`, `DEMOILEn.DAT`) |
| `0x517f04` | island-exit request: 0 = none, 2..5 = go to that island, 6 = show ending movies |
| `0x517f0c` | lives (3 at start; loop runs while `>= 0`) |
| `0x50d5e0` | menu result: continue with a loaded game (skip reset) |
| `0x50d5e1` | menu result: quit to DOS |
| `0x50d5e2` | "new game" flag → `game_reset(0)` |
| `0x40014` | UNI loaded flag |
| `0x40058` | view (re)init state: 1 = need mode switch + fade in, 3 → 2 → 0 |
| `0x4002c` | view mode: 0 = 320×200 mode 13h, 1 = 320×200 window centred in 640×480, 2 = 640×400 (default 2) |
| `0x40004` / `0x40008` | sound enabled / CD enabled (both 1 in the binary) |
| `0x1c6ee4` | CD drive index (0 = A); patched into `C:\PUNKY\...` paths |

## Flow

```
sys_init()                                  // 0x11ef0: CD detect, sound, samples
MENU:
  vesa_init(0x101, 640); gfx_init_rows(640); vesa_set_mode(0x101); set_clip(0,0,639,479)
  memset(black_palette 0x197dd0, 0, 0x300)
  menu_run()                                // 0x25b00, CD track 4
  if (quit) { text mode 3; cd_stop; sound_shutdown; flush keys; exit(0) }
NEWGAME:
  if (new_game) { game_reset(0); new_game = 0 }
  if (!continue_loaded) {
      uni_init("X:\PUNKY\ILE_n.UNI")        // if not loaded
      island_reset[n]()                     // table 0x415b8
      game_reset(1)
      ileNN_trj_load()                      // per island
  }
ISLAND:
  island_enter(n)                           // 0x11fc0: mode 13h, per-island asset loader
  load_ron("Animaps.RON" → 0x21c5c8, pitch 192, palette 0x489690)
  load_ron("Ciel_n.RON"  → 0x1980d0, pitch 640, palette 0x4f0b00)
  col_load("ile_n.col", 0x4f0b00)           // island palette replaces the sky's
  palette_set(0,255, black); g_40058 = 1
  kbd_install(); input_poll(0); sfx_play_loop(3 /*mer11.pcm*/, 0)   // sea ambience, vol 0
  demo_open(); cd_stop(); if (music) cd_play_track(5)
  while (!key[ESC] && lives >= 0 && exit_request == 0) {
      mouse_read()
      while (mouse_buttons == 1) { kbd_remove(); wait release; kbd_install() }   // pause on click
      game_tick_frame()                     // 0x2a260 (logic, see timing.md)
      island_frame[n]()                     // 0x187e0 / 0x1a6d0 / 0x1c450 / 0x1e080 / 0x173d0
      shop_frame()                          // 0x2a450 (returns at once unless at merchant)
  }
  if (exit_request == 2) exit_request = 6   // !!! see below
  demo_close(); uni_loaded = 0
  if (exit_request == 6) {                  // ending
      cd_stop(); cd_play_track(7) (twice)
      palette_fade(0,255,32, 0x489690 → black); vesa_set_mode(0x101); ...
      flc_play("part_1.mov", 15); flc_play("part_2.mov", 1)
      exit_request = 0; lives = -1; cd_stop(); kbd_remove(); goto MENU
  }
  if (exit_request != 0) {                  // 3,4,5 (2 can never get here)
      show_picture("TRANS{1-2,2-3,3-4,4-5}.RON", 150)
      island = exit_request; continue_loaded = 0
      demo_close(); kbd_remove(); input_poll(0); goto NEWGAME (without reset)
  }
  // ESC or out of lives
  demo_close(); kbd_remove(); input_poll(0)
  game_over_screen()                        // 0x255f0 ("CONTINUE 9")
  if (!new_game_flag) goto MENU
  g_517f20 = 99; g_517f18--; lives = 3; goto ISLAND      // continue
```

### The island 1 → ending shortcut — **verified**, meaning **hypothesis**

At `0x124ea`: `cmp [0x517f04],2 / jne / mov [0x517f04],6`. Leaving island 1 (the raft
section `0x178d0` sets request 2) therefore plays `part_1.mov`/`part_2.mov` with CD
track 7 and returns to the menu; the `TRANS1-2.RON` branch for request 2 is unreachable.
Islands 2–5 are still fully present in code and data and reachable through the cheat
menu (`0x27880` sets the island directly) or a saved game. Either this build is a
one-island preview/demo cut, or `PART_n.MOV` is an intended "end of part one" break —
check the reference capture and `PART_1.MOV` content before deciding what the port does.

## Per-island frame driver (island 1: `0x187e0`) — call order **verified**

`0x188e0` (pre), `0x178d0` (scripted events / vehicle), `0x185a0` (inventory screen),
`0x11b40` (animated tiles), `0x18160`, `0x184a0`, `map_screen("Carte01.RON","Carte11.RON")`
(`0x280f0`), a list of `object3d_place(...)` calls for static scenery, then
`render_frame()` (`0x148a0`). Screens such as map, inventory, dialogue (`0x28e00`) and
shop are modal functions with their own loops, entered from inside the frame; they
restore the view by setting `0x40058 = 1`.

## Menu (`0x25b00`–`0x27a00`)

Pages are functions named after their title strings (see `functions.md` §2). Selection
dispatch is via jump tables (e.g. `0x25ea4`). The main menu has 4 entries without a game
in progress (new, load, options, exit) and 6 with one (`0x50d5e3`: adds continue and
save). CD track 4 plays in the menu; the sound page re-triggers it.

## CD tracks used by P.EXE — **verified** call sites of `cd_play_track`

| Track | Where |
|---|---|
| 4 | menu (`0x25b58`, `0x26b3c`) |
| 5 | island play (`0x1243a`), restarted by `cd_loop_check` (`0x11fac`) and `0x1192a` |
| 6 | per-island section functions `0x18160`, `0x1a200`, `0x1be60`, `0x1dbb0`, `0x16f90` (boss or vehicle section — **hypothesis**) |
| 7 | ending movies (`0x1251f`) |

Tracks 2, 3, 8, 9 are not referenced by P.EXE (INTRO.EXE / THE_END.EXE not analysed).
