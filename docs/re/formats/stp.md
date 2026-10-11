# `GAME.STP` — saved settings

Parser/writer: `crates/pd-formats/src/stp.rs`. Dump: `game_stp.json`.

32 bytes, 8 x little-endian u32, read at `0x258c4` (mode `rb`) and written at `0x25a40`
(mode `wb`) with one `fread`/`fwrite` of 4 bytes per variable, in this order. **verified**
No range checks on load. Field meanings come from the menus that edit the variables.

| # | Variable | Meaning | Evidence |
|---|---|---|---|
| 0 | `0x41610` | sound: 0 off, 1 on | "SOUND AND MUSIC" menu `0x26989`, strings `SOUND OFF/ON` at `0x4183c` — **verified** |
| 1 | `0x41614` | music: 0 off, 1 on | same menu, `MUSIC OFF/ON` at `0x41844`; gates CD track start at `0x12431` — **verified** |
| 2 | `0x41618` | controller: 0 keyboard, 1 joystick | "CONTROLLER" menu `0x26bc8`, strings at `0x41864`; reset to 0 when joystick detection fails (`0x27395`) — **verified** |
| 3 | `0x4161c` | SFX volume | bar in `SFX VOLUME : ,,,,,,,,,,,,,,,,` (`0x269c5`) — **verified**; range 0..15 from the 16-slot bar — **hypothesis** |
| 4 | `0x41620` | CD volume | bar in `CD VOLUME : ,,,,…` (`0x269f0`) — same status |
| 5 | `0x41624` | difficulty: 0 `EASY !`, 1 `MEDIUM !!`, 2 `HARD !!!` | "OPTIONS" menu `0x26669`, table `0x417ec`; scales monster stats at `0x2417f` — **verified** |
| 6 | `0x416e4` | language: 0 French, 1 English, 2 Spanish, 3 German, 4 Polish | table `0x417d4` (`0x2667a`); order matches the 5 lines per entry of `ILE_n.TXT` — **verified** |
| 7 | `0x4000c` | game speed, signed, -6..6 | `GAME SPEED : .............` slider, clamps at `0x26723`/`0x2673f` — **verified** |

Disc file: `1, 1, 0, 15, 5, 1, 1, 0` = sound on, music on, keyboard, SFX 15, CD 5, medium,
English, speed 0.

The resolution menu (`VGA 320X200`, `SVGA 320X200`, `SVGA 640X400`, `FAST MODE`) is not
saved here. The same routine also loads `SAVEGAME.REC` (8 slot names, default `FREE 1`..`FREE 8`).
