# Input (P.EXE)

**Verified** in disassembly unless marked.

## Keyboard

`kbd_install` (`0x31b90`) hooks int 9 with `kbd_isr` (`0x358f8`); the BIOS handler is
not chained. `g_keys[256]` at `0x51d1f0`: index = scancode, `+0x80` for `E0`-prefixed
keys; value `0xFF` while down, 0 when up. `input_poll` first masks every entry with 1
(so "down" reads as 1) and counts keys down. `E1` (Pause) toggles `0x51d304`.

## Input record — 16 bytes at `0x50d5c4`

Built by `input_poll(gameplay)` (`0x26ec0`); this exact record is what
`DEMOILEn.DAT` stores, one per simulation tick (`demo_step`, `0x25070`).

| Byte | Source (scancode) | Kind | Use |
|---|---|---|---|
| 0 | Up (`E0 48`) | level | forward |
| 1 | Down (`E0 50`) | level | back |
| 2 | Left (`E0 4B`) | level | turn |
| 3 | Right (`E0 4D`) | level | turn |
| 4 | Ctrl (`1D` or `E0 1D`) | level | fire/action |
| 5 | Enter (`1C`) | edge | toggle timed item (`0x11960`) |
| 6 | Backspace (`0E`) | edge | next weapon/item |
| 7 | F1 (`3B`) | edge | cycle view mode |
| 8 | F2 (`3C`) | edge | toggle interlace (view mode 2 only) |
| 9 | Space (`39`) | level | jump (**hypothesis**); also forced by the idle timer |
| 10 | Tab (`0F`) | edge | (inventory/map — **hypothesis**) |
| 11 | any of `E0 47/49/4F/51/52/53` (Home, PgUp, End, PgDn, Ins, Del) | edge | |
| 12 | Esc (`01`) | edge | also set at demo EOF |
| 13 | `1` (`02`) | edge | |
| 14 | `2` (`03`) | edge | |
| 15 | – | | |

Bytes 0–6, 9–11, 13–14 are only filled when `gameplay == 1`. Edge detection uses the
previous-state bytes at `0x50cbb9`–`0x50cbc2`. With the joystick selected
(`0x41618 == 1`) bytes 0–3 come from an 8-way direction code `0x41948` (1 = up, 2 = up+
right, 3 = right … 8 = up+left; other values fall back to OR-ing the keyboard), buttons
from `0x4194c`/`0x41950`.

Other keys handled in `input_poll`: `P` (`19`) and Pause wait for release and then
block until pressed again (pause); F12 (`58`) writes `SCRSHTnn.RAW` (32-byte `mhwanh`
header, 768-byte 8-bit palette, 256000 bytes = 640×400 back buffer) and flashes the
palette. Exactly three keys held triggers the cheat-code checks (flags in `0x4191a`;
first `H`+`C`+`T` to arm) — gameplay effects are in `game/save-and-settings.md`.

## Joystick

Game port `0x201`. `joy_buttons` (`0x27a30`): bits 4–7 inverted → `0x4194c`, `0x41950`,
`0x41964`, `0x41968`. `joy_detect(axis)` (`0x27ab0`) fires the one-shots (`out 0x201`),
busy-waits 8000 port reads and flags the axis absent if its bit is still set.
`0x27b10`/`0x27c70` time the axes and compare with calibration (menu pages `0x26c70`,
`0x26d10`) to produce the direction code — internals **hypothesis**.

## Mouse

`mouse_read` (`0x3350e`): int 33h AX=3 → x `0x5337dc`, y `0x5337e0`, buttons
`0x5337e4`. In the island loop a left click pauses until release. The shop
(`0x2a450`) uses rectangular hot zones: `0x2b370` reset(n), `0x2b3a0` set(i,x0,y0,x1,y1)
into `0x517fa4` (16 bytes each), `0x2b400` hit-test(x,y) → index or −1. Cursor
draw/restore helpers live at `0x31dd0`–`0x32300` (**hypothesis**).
