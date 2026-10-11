# Reference runs of the original (DOSBox-X)

The original runs in DOSBox-X 2026.10.01, headless, with scripted input and A/V capture.
Use it to settle anything static analysis cannot (timing, behaviour, what a screen looks like).

```sh
tools/dosbox/run.sh                      # interactive, runs DUCK.BAT (intro, game, ending)
tools/dosbox/run.sh --headless --limit 70 --capture \
    --keys "-w 15 -p 2 enter enter enter right right space" P.EXE
```

- Config: `tools/dosbox/punkyduck.conf` (S3 SVGA, Pentium, fixed 30000 cycles, SB16 at 220/5/1/5).
- `disc/dosbox/c/PUNKY` is a writable copy of the extraction, mounted so the game sees `C:\PUNKY`;
  the cue image is mounted as `D:` for CD audio.
- `--capture` wraps the command in `DX-CAPTURE /V /A`; output lands in `disc/dosbox/capture/`
  as ZMBV `.avi` (lossless, one file per video mode change) and `.wav`. ffmpeg reads both:
  `ffmpeg -i p_001.avi -vf "select='not(mod(n\,300))',tile=4x3" -frames:v 1 sheet.png`.
- `--keys` passes arguments to DOSBox-X `AUTOTYPE` (`-w` seconds before first key, `-p` seconds
  between keys). Keys are tapped, not held, so sustained movement needs repeated entries.
- Log goes to stdout/stderr; `CDROM: Playing track # N` lines show music changes.
- Several instances may run at once, but they share `disc/dosbox/capture/` and the writable
  `C:` copy (GAME.STP gets rewritten). Copy captures you want to keep elsewhere under `out/`.

## Observed so far (**verified** by capture)

- P.EXE starts without SETUP; the shipped `SOUNDRV.COM` is byte-identical to `SETUP/NOSOUND.COM`,
  so sound effects are off until a DIGPAK driver from `SETUP/` is configured (not done yet).
  CD music plays regardless.
- Video: 640x480, 8-bit, about 70.0 Hz refresh. BIOS mode `0x69` is logged at mode set
  (S3 number for VESA 640x480x256).
- Main menu: MENUSPIC picture, dimmed box, items `NEW GAME`, `LOAD GAME`, `OPTIONS`, `EXIT`.
  CD track 4 plays on the menu.
- `enter` x3 from the menu reaches island gameplay; CD track 5 starts.
- Gameplay view is **not** top-down: a 3D terrain seen in perspective from behind the duck, with
  a sky band above the horizon. The terrain is the 256x256-cell heightmapped tile map from
  `ILE_n.UNI` (see `formats/uni.md`), not a flat bitmap. The duck, enemies and pickups are
  sprites standing on it, scaled by depth. The playfield occupies a 640x400 band inside 640x480.
- HUD: duck face + percentage + hearts top-left, 7-digit score top-right, item slot and a bar
  bottom-right. Enemies fire projectiles; hits show explosion sprites and the percentage drops.

## Deterministic state comparison (per-step input recording, scripted save and load)

Used by `crates/pd-game/tests/sim_dosbox.rs` to compare the gameplay port with the original
byte for byte. **Verified**: six runs (walking, turning, firing, drowning, death, respawn,
pickup, a loaded edited save) give save games identical to the port's in all 181 817 bytes.

The idea: the original can record its 16-byte input record once per simulation step
(`DEMOILEn.DAT`, `game/save-and-settings.md` 3) and a save game is a full state dump. Two
changes to a **private copy** of the game make a run reproducible:

1. Record flag on: data byte `0x41600 = 1` (file offset `0x47200 + 0x1600`; the data object
   `0x40000` starts at file offset `0x47200`), and the store that clears it when the menu
   opens neutralised: at code `0x25b18` (file offset `0x19200 + 0x15b18 = 0x2ed18`) change
   opcode `88 25` (`mov [0x41600],ah`) to `38 25` (`cmp`). Same length, the relocated
   operand is untouched.
2. `GAME.STP` field 7 (game speed) `= -6`: `ticks = elapsed - 6`, floored to 1, so every
   frame that takes at most six 70 Hz ticks runs exactly two steps. Raise the cycles
   (`cycles=fixed 150000`) so that holds. (Clearing the sound flag `0x40004` instead does
   not work: the game hangs entering the island.)

Frames that follow a palette fade still run more steps because the counter keeps running
during the fade: the first frame of an island (60 or 61 steps seen) and the frame after a
respawn (18 seen). The test searches those two numbers; everything else is fixed.

Script (working copy `out/simcmp/run.sh NAME LIMIT "AUTOTYPE ARGS" [save-to-preload]`,
private `C:` under `out/simcmp/NAME/c`, own capture directory, does not touch
`disc/dosbox`):

```sh
dir=out/simcmp/$name; rm -rf "$dir"; mkdir -p "$dir/c" "$dir/capture"
cp -r disc/extracted/PUNKY "$dir/c/"
python3 - "$dir/c/PUNKY/P.EXE" <<'PY'
import sys, struct, os
p=sys.argv[1]; d=bytearray(open(p,'rb').read())
d[0x47200+0x1600]=1                       # demo_record
assert d[0x2ed18:0x2ed1a]==bytes([0x88,0x25]); d[0x2ed18]=0x38
open(p,'wb').write(d)
g=os.path.join(os.path.dirname(p),'GAME.STP'); v=list(struct.unpack('<8i',open(g,'rb').read()))
v[7]=-6; open(g,'wb').write(struct.pack('<8i',*v))
PY
# optional: cp edited.sav "$dir/c/PUNKY/SAVEGAME.__0"; cp some/SAVEGAME.REC "$dir/c/PUNKY/"
sed -e "s#^captures=.*#captures=$dir/capture#" -e "s#^cycles=.*#cycles=fixed 150000#" \
    tools/dosbox/punkyduck.conf > "$dir/dosbox.conf"
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy timeout -s KILL $((limit+20)) dosbox-x \
  -conf "$dir/dosbox.conf" -nomenu -fastlaunch -c "mount c $dir/c" \
  -c "imgmount d disc/PUNKYDUCK.cue -t cdrom" -c "c:" -c "cd \\PUNKY" \
  -c "AUTOTYPE $keys" -c "P.EXE" -c "exit" -time-limit "$limit"
```

Key scripts (AUTOTYPE):

- New game, play, save to slot 1:
  `-w 12 -p 0.5 enter enter enter <play keys> esc down down down enter enter enter enter`
- **Load** slot 1, play, save: `-w 12 -p 0.5 down enter enter down enter <play keys> esc down
  down down enter enter enter enter`. The load only works when `SAVEGAME.REC` exists next to
  `SAVEGAME.__0` (any REC written by an earlier run will do, even with the slot still named
  `FREE 1`); without it the confirmation never loads and the file comes back unchanged
  (this was the failure noted earlier in `game/open-questions.md`).
- Things that silently break a script: an unknown key name stops AUTOTYPE on the spot
  (`backspace` is not a valid name: log line `MAPPER: Couldn't find a button named ...`),
  sometimes leaving the previous key held; `-time-limit` did not end the run reliably,
  hence the outer `timeout -s KILL`; a run killed before the game closes the demo file
  loses its last partial 4096-byte block, so always end with the save sequence.
- Keys are taps (2 to 4 steps each at this speed). The demo file tells exactly what the game
  saw, so the script does not need to be timed precisely.

After the run: `DEMOILE1.DAT` (16 bytes per step: `engine/input.md`) and `SAVEGAME.__0`.
To use an edited save as the start state, patch the fields listed in
`game/save-and-settings.md` 2 (e.g. `px` at 112, `pz` at 120, `tx` at 124, `tz` at 132).

Observed with this method (**verified**): the random seed is 1 at program start and island 1's
random money layout follows from it; path monsters drift as coded (`game/entities.md` 3.6);
the HUD takes a second life on death (`game/player.md` 4).
