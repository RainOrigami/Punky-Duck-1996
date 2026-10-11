# Menus, game-over screen, modal screens, intro and credits

Status legend as in `README.md`. Addresses are `P.EXE` link addresses unless a section says
otherwise. "Capture" = lossless DOSBox-X capture compared pixel by pixel with the port's
headless frame dumps (runs under `out/app-ref/`, scripts `out/app-ref/run.sh`,
`segments.py`, `compare.py`). Port code: `crates/pd-game/src/app/`.

## 1. Menu engine

### `menu_run` (`0x25b00`) — **verified** (decompiled; main menu pixel-exact in capture)

```
quit = new_game = resume = 0; demo_record = 0
settings_load()                         // 0x258c0: GAME.STP, SAVEGAME.REC
flush BIOS keys
mixer master = sfx_volume << 4; mixer_on = sound_on
cd_stop(); if (music_on) cd_play(4); cd_set_volume(cd_volume << 4)
joy_detect(0) ...                       // sets 0x4162c / 0x4193c (joystick absent)
palette_set(black)
load_ron("MENUSPIC.RON" -> 0x2c4498, palette 0x50cbc4)
blend_table(0,0,0, 16, palette -> 0x50cab4)       // 0x252d0, the box darkening table
load_banque COOLFNT1 -> 0x555d0, COOLFNT2 -> 0x72a90, COOLFNT3 -> 0x79fc0
font_set(COOLFNT2, 0x20, 0); sprite target = back buffer, clip 0..479
menu_main()                             // 0x25ed0
settings_save()                         // 0x25a40: GAME.STP only
```

### `menu_draw(title, items, sel)` (`0x25470`) — **verified** (pixel-exact, 30 screens)

- `n` = number of items (NULL-terminated pointer list), `h = n*40 + 110`, `top = -(h/2)`.
- Copy the background picture to the back buffer, then darken a centred box of 400 x `h`
  through the table (`0x253d0`): columns 120..519, rows `240 - h/2 + 1 .. 240 - h/2 + h - 1`
  (the loop skips the first row).
- Items: centred at x = 325, y = `i*40 + top + 360`, drawn with `COOLFNT3` (green); with
  `sel < 0` they are drawn with `COOLFNT1` instead (message pages).
- Title: `COOLFNT2` (orange) at (325, `top + 280`).
- If `sel >= 0` the selected item is drawn again with `COOLFNT1` on top.
- The sheet pointer (`0x51e460`) is swapped directly, so glyph limit (59, from `COOLFNT2`) and
  spacing 0 of the last `font_set` apply to all three fonts: `COOLFNT1` characters above
  `0x5B` never appear in menus.
- Whole back buffer to the screen (`vesa_blit(.., 0x280, 0x1e0, ..)`).

`blend_table(r,g,b,amount,pal,out)` (`0x252d0`): for each colour the nearest palette entry
(squared RGB distance on the 6-bit values, later index wins ties, start value `0xb98c0`) to
`c + (target - c) * amount / 32` (truncating). **verified**

The sprite blitter draws row `r` of a glyph on line `y + hot_y + r + 1` (`functions.md`); with
that the text lands exactly where the capture has it.

### `menu_input(&sel, count)` (`0x25c90`) — **verified**

```
ascii = scan = 0
while (bios_kbhit()) bios_getkey()      // int 16h; the last key waiting wins
if (ascii + scan) kbd_flush()
if (scan == 0x48) sel--;   if (sel < 0) sel = count - 1
if (scan == 0x50) sel++;   if (sel == count) sel = 0;   if (sel < 0) sel = 0
if (ascii == 0x1b) { sel = count - 1; ascii = 0x0d }      // Esc = choose the last entry
if (scan is 0x50, 0x4d, 0x4b or 0x48) sfx_play(11, 255)
if (ascii == 0x0d) sfx_play(12, 255)
if (ascii && !cheat) {                                    // level-select cheat
    shift the 22-byte buffer 0x416ca left, append ascii
    if (strncmp(buffer, "THE TRUTH IS OUT THERE", 22) == 0) {
        cheat = 1                                         // 0x416e0
        palette_fade(4,  menu palette -> red palette)     // red = (r+g+b)/3, g = b = 0
        palette_fade(16, red palette -> menu palette)
    }
}
```

The compare is case sensitive (`0x35a00` is `strncmp`), so the phrase has to be typed in
capitals. Keys come from the BIOS, so held keys repeat at the keyboard's typematic rate
(the port uses the AT defaults, 500 ms and 10.9/s — **hypothesis** for what a given PC did).

### Loop rate — **verified** (measured), machine dependent

Every page is `for (;;) { menu_draw(); menu_input(); act }` with no wait: the pass rate is
whatever the full-screen blit allows. On the reference machine the cursor of the name entry
page (toggles every 4 passes) alternates 16 and 17 retraces, i.e. **33 retraces per 8 passes**
(59 ms per pass). The port uses exactly this rate (`menu.rs`, `PASS_NUM/PASS_DEN`); it sets the
`CONTINUE` countdown speed and the cursor blink.

### `option_cycle(item, names, &value, dir, n)` (`0x25e60`) — **verified**

`value = (value + 1) % n` for `dir >= 0` (the only use), and the item's string pointer is
replaced by `names[value]`.

## 2. Pages — all **verified** (decompiled); "capture" marks pixel-exact screens

| Page | Function | Title / items | Behaviour |
|---|---|---|---|
| Main | `0x25ed0` | `MAIN MENU`; `NEW GAME`, `LOAD GAME`, [`RESUME GAME`, `SAVE GAME`,] `OPTIONS`, `EXIT` (tables `0x41714`, `0x41728`) | Draw with `sel = 0`, fade in 16 steps, `g_ticks = 0`, loop until new game, resume or quit is set; then fade out 16 steps. `RESUME GAME` sets `0x50d5e0 = 1`. After every chosen entry `g_ticks = 0`. Capture (4 and 6 entries). |
| New game | `0x27880` | – | Without the cheat: island 1, `0x41628 = 1`, new game. With it: `CHEAT MODE`; `FISHER ISLAND`, `OASIS ISLAND`, `YETI ISLAND`, `COLD ISLAND`, `SMOKE ISLAND`, `CANCEL` (`0x41920`). |
| Load | `0x264f0` | `LOAD GAME`; 8 slot names, `CANCEL` (`0x4168c`) | Slot → confirm page → `savegame_read`; on success `trj_load_monsters`, `0x24650`, resume = 1, `0x41628 = 1`. Capture. |
| Load confirm | `0x26460` | `LOAD <name> ?`; `NO`, `YES` (`0x417c8`) | Loops until Enter; returns the index. Esc lands on `YES` (last entry). Capture. |
| Save | `0x265c0` | `SAVE GAME`; 8 slot names, `CANCEL` | Slot → name entry → `savegame_write`. Stays on the list afterwards. Capture. |
| Name entry | `0x26280` | `ENTER NEW NAME`; one item: name + cursor | See below. Capture (both cursor phases). |
| Save result | `0x26100` | `OK !`; `GAME SAVED`, `SUCCESSFULLY !` (`sel = -1`) | Waits for a key (`0x359a3`), then writes `SAVEGAME.REC`. On `fopen` failure: `!!! ERROR !!!`; `GAME NOT`, `SAVED !`. |
| Load error | `0x26050` | `!!! ERROR !!!`; `GAME NOT`, `LOADED !` | When the slot file cannot be opened. Waits for a key. |
| Options | `0x26660` | `OPTIONS`; difficulty, language, `GAME SPEED : ` + 13 bar characters, `RESOLUTION OPTIONS`, `SOUND OPTIONS`, `CONTROLLER OPTIONS`, `RETURN` | Enter cycles difficulty (3) / language (5), on the speed line adds 1 (max 6); Left / Right on the speed line change it within −6..6. Bar: characters 13..25 are `*` (0x2a), the first `speed + 7` of them `+` (0x2b); both are bar glyphs in the fonts. Capture. |
| Resolution | `0x26840` | `RESOLUTION`; `VGA 320X200` / `SVGA 320X200` / `SVGA 640X400`, `FAST MODE OFF` / `ON`, `RETURN` | View mode `0x4002c`, interlace `0x40044`. Leaving 640x400 turns fast mode off; turning fast mode on selects 640x400. On return `0x40054 = 0x40044`. Not saved. Capture. |
| Sound | `0x26980` | `SOUND AND MUSIC`; `SOUND ON/OFF`, `MUSIC ON/OFF`, `SFX VOLUME : ` + 16, `CD VOLUME : ` + 16, `RETURN` | Enter toggles / adds 1 (`& 15`); Left / Right change a volume within 0..15. Toggling music: `cd_stop`, `cd_play(4)` if on. Mixer switch, master volume and CD volume are re-applied every pass. Capture. |
| Controller | `0x26bc0` | `CONTROLLER`; `KEYBOARD` / `JOYSTICK`, `CALIBRATE JOYSTICK`, `RETURN` | On return the controller goes back to keyboard if no joystick was detected (`0x4193c`). Capture. |
| Calibrate | `0x26c70` | `CALIBRATE JOYSTICK`; `NO JOYSTICK`, `DETECTED !`, `PRESS A KEY...` | Without a joystick. With one: `LEAVE YOUR JOYSTICK CENTERED AND PRESS FIRE BUTTON !`, then the test page `0x26d10` (`JOYSTICK TEST`). The port has no game port and always shows the first. |
| Exit | `0x26e30` | `EXIT GAME ?`; `HO NO!`, `YES, SORRY...` | Capture. |

### Name entry (`0x26280`) — **verified**

The item is the slot's current name followed by a cursor character. Per pass:
`count++`; `menu_input(&sel, 0)`; lower case → upper case; cursor character =
`'+' - ((count & 4) >> 2)` (two bar glyphs); draw; Backspace (scan `0x0e`) removes the last
character if the string is longer than the cursor alone; a character of
`a-zA-Z0-9` or space is appended while the name is shorter than 8. Enter ends, Esc cancels
(the only way `sel` becomes negative). An empty name becomes `sprintf("ISLAND%#02d", island)`.
The name is copied into the 10-byte slot name only when not cancelled.

### Files — **verified** (`GAME.STP` byte-identical to the original's for the same key sequence)

`GAME.STP` is read when the main menu opens and written when it closes (also on quit).
`SAVEGAME.REC` (8 x 10 bytes) is read with it and written only after a successful save.
The slot file is `SAVEGAME.__n`; its last four bytes are the difficulty, which the load puts
into the option variable.

## 3. Game-over screen (`0x255f0`) — **verified** (decompiled; not captured)

```
flush keys; new_game = 0
if (0x41628 == 0)      { in_progress = 0; lives = -1; return }     // attract mode
if (lives >= 0)        { in_progress = 1; return }                 // Esc during play
in_progress = 0
mode 0x101, palette black; load_ron("GO_n.RON") as menu background and palette
fonts, font_set(COOLFNT2, 0x20, 0), flush keys
if (continues == 0) { show the picture; fade in 16; do menu_input(&sel, 1) while (!ascii) }
else {
    count = 599; menu_draw("CONTINUE 9", {YES, NO}, sel); fade in 16
    do { title[9] = '0' + count / 60; menu_draw(...); count--; menu_input(&sel, 2)
         Enter: YES -> new_game = 1 (continue), NO -> 0; count == 0 -> 0 } ...
}
fade out 16
```

The darkening table is **not** rebuilt for `GO_n.RON`'s palette; the box uses the table made
for `MENUSPIC.RON` (kept in the port: `MenuGlobals::dim`).

## 4. Modal screens during play

All **verified** by decompilation; "capture" where compared.

| Screen | Function | Sequence |
|---|---|---|
| Picture | `0x28750` | `input_poll(0)`; fade 10 island palette → black; mode 0x101; palette black; `load_ron(name)` to the back buffer; `vesa_blit(.., 0x27f, 0x1df, ..)` (last column and row of the screen keep colour 0); fade 10 in; wait until any input, then until none (`input_any`, `0x2a2f0`); fade 10 out; fill the screen with 0xfe; `0x40058 = 1`; `g_ticks = 0`. |
| Picture, given speed | `0x28870` | same with `steps` instead of 10 (the island transitions use 150). |
| Timed picture | `0x28990` | same, but instead of the input wait: `delay(100)` (Watcom `delay`, milliseconds; `0x37e83`). With the argument 16 it is on screen at full brightness for 100 ms between two 17-retrace fades. |
| Dialogue | `0x28e00` | like Picture with `DIAL_n.RON` (`DIAL_51.RON` on island 5 in the outfit); the four strings of the line (`0x25140`) are drawn centred at x = 320, y = 100, 130, 160, 190 with the island font (`COOLFNT1`, spacing −5). |
| Text on picture | `0x290e0` | Dialogue with a given picture. |
| Inventory | `0x185a0` (island 1) | `INV_0n.RON`; item `i` of the inventory: sprite `id + 8` of `ICONE_n.SPT` at (`(i%4)*100 + 70`, `(i/4)*100 + 90`); 16 frames (`rect_outline`, colour 0xfe) at (`c*100 + 30`, `r*100 + 50`)–(`+110`, `+130`); waits for Space (input byte 9). Capture. |
| Map | `0x280f0` | `sections.md` 2. The explored test reads `0x2456b0[cx*256 + cz]`, 8 bytes into the saved array. Markers and player are drawn straight to the screen. Compass triangle: tip `(sx + 7cos a, sy + 7 sin a)`, other corners `(sx − 7cos(a ∓ 0.7), sy − 7 sin(a ∓ 0.7))`, truncated; lines by `0x32d80`/`0x32a5c` (line algorithm not read: **hypothesis** that it is plain Bresenham). Capture without compass. |
| Shop | `0x2a450` | `MARCHAND.RON` via `0x28ab0`; undo copies; 22 hot zones (BUY `(28,274)-(127,294)`, CANCEL `(28,312)-(127,331)`, EXIT `(14,437)-(93,459)`, products in rows at y 20–89 (8), 105–174 (7), 192–261 (4), 72 pixels apart from x 17 / 161); cursor = sprite 0 of `HAND.SPT`, starts at (320, 240), moved 2 pixels per retrace by the arrow keys within 8..599 x 8..441; Space "clicks" the zone under it; Esc or EXIT leaves. Each pass: money as `%d` centred at (342, 452), the selected product's `BONUS_n.SPT` frame at (320, 368), the hand; the three touched areas are presented and restored. Messages (`0x2a320`): two lines at (510, 204) and (510, 220), shown and removed four times with `wait_hretrace(4000)` in between. |

View re-initialisation after any of them (`0x40058 = 1`, `render_frame` `0x148a0`,
`view_mode_apply` `0x146f0`): the next frame is rendered but not shown, the palette is faded
"out" over 20 steps, the screen filled with 0xfe; the following two frames are shown with a
black palette and the second is followed by a 20-step fade-in and `g_ticks = 0`.
**verified**: the port's frame and fade counts after NEW GAME equal the capture's (17 black
retraces, 20 fade steps, then one picture per frame), and the first 14 island frames are
pixel-identical.

## 5. `INTRO.EXE` — **verified** (decompiled; timing against the existing capture)

Addresses in `INTRO.elf`. `main` = `0x103d0`, `play(name, hold_ms, track)` = `0x10010`.

```
play("C_SOFT.MOV", 200, 2)
skipped = 0
play("PUNKY.MOV", 1000, -1)
if (!skipped) play("INTRO.MOV", 200, 3)
if (!skipped) palette_set(black)
cd_stop
```

- Timer: PIT divisor `0x4a9` (1000 Hz) through int 1Ch (`0x11370`), counter `0x20150`.
- `play`: open; for each frame: on frame 0 `cd_stop, cd_play(track)` twice if `track >= 0`;
  if a key is waiting (int 21h AH=0Bh, `0x1240c` — the key is **not** read) the loop ends
  after this frame and `skipped = 1`; decode; frame-speed table (`0x6ba90`, only filled for
  `INTRO.MOV`: frame 0 → 8, 416 → 7, 441 → 8; speed = `trunc(n * 14.28)` ms from then on, i.e.
  114, 99, 114); wait (`0x113c0`): a frame that is already later than its due time is not
  shown, otherwise wait until `speed` ms after the previous frame; the first frame
  re-anchors the schedule. Shown frames: palette (frame 0: fade in from black over 32 steps,
  else set), blit centred. After the loop: `delay(hold_ms)`, fade colours 1..255 to black
  over 100 steps, clear.
- Because the key stays in the buffer, one key press ends `C_SOFT.MOV`, lets `PUNKY.MOV`
  show a single frame (with its hold and fade), and skips `INTRO.MOV`.
- Measured: with this model the port shows each movie picture within −7..+3 retraces of
  the capture over the whole 80 s (constant −4/−5 during `INTRO.MOV`, which is the file
  loading time of the original), including the faster stretch of frames 416–440 that
  `formats/flc.md` lists as unexplained.

## 6. `THE_END.EXE` — **verified** by decompilation only (no capture yet)

Addresses in `THE_END.elf`. `main` = `0x10170`: flush keys, CD track 8, mode 0x101,
`MENUSPIC.RON` as background, two tables from `blend_table` (towards black and towards
white, amount 16), the three `COOLFNT` sheets, sprite clip rows 0..470, then `0x10340`:

```
for each *START* .. *STOP* page of INTRO.INI (0x10410; lines read with fgets(32), CR LF cut,
                                               at most 16 lines):
    background to the screen; fade in 64 steps
    aborted = page()                    // 0x10540
    fade out 128 steps
    if (aborted) break
```

`page`: lines are 40 pixels apart, the block centred vertically (`(480 − 40 n) / 2`). Empty
lines only take their row. For each other line, once per retrace:

1. Fly-in, `v` = 200, 198, … 2: restore rows `y .. y+40` of the background
   (`y = row + v`), draw the text centred at (325, `y + 30`) with
   `font_set(sheet, 0x20, v)` — the step value is the **letter spacing** — and present those
   rows. First line of a page: `COOLFNT2`; others: `COOLFNT1`.
2. Band, `s` = 0, 8, … 312 and once more at full width: restore the rows, remap
   `x = 0 .. 2s` (first line, towards black) or `x = 639 − 2s .. 639` (other lines, towards
   white) of rows `y .. y+39` through the table, draw the text with spacing 2, present.

A key (int 16h) during 1 or 2 ends the page and the program. After the last line:
`delay(8000)`.

## 7. Ending inside `P.EXE` (`0x12509`) — **verified** (decompiled)

`cd_stop, cd_play(7)` twice; `palette_fade(0, 255, 32, ANIMAPS.RON's palette (0x489690) → black)`;
mode 0x101; palette black; `flc_play("part_1.mov", 15)`; `flc_play("part_2.mov", 1)`;
`next_island = 0; lives = -1; cd_stop`; back to the menu.

`flc_play(name, loops)` (`0x2bc70`): `n = trunc(speed / 14.28)` retraces before each frame
(7 for both files); `loops * frames` frame reads; the reader (`0x2c1e0`) wraps through the
ring frame to the second frame, so a 12-frame file played 15 times shows 180 pictures
(18 s), then the screen is cleared. Not skippable.
