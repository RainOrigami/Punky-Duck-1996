# Building and running

Needs a current stable Rust toolchain (`rustup`), plus the `wasm32-unknown-unknown` target
and `wasm-bindgen-cli` for the web build. All commands run from the repository root.

You need your own disc image: `PUNKYDUCK.bin` + `PUNKYDUCK.cue` (kept in `disc/`,
gitignored), or the extracted `PUNKY` directory.

Both frontends run `pd_game::app::Game`: the whole of `DUCK.BAT` — the intro movies of
`INTRO.EXE`, the game (`P.EXE`: menus, islands, ending) and after EXIT the credits of
`THE_END.EXE`. The engine demo (`pd_engine::demo::DemoApp`: movies, then a test pattern
showing the input state) is still available with `--demo` (native) or `?demo` (web).

**Original / enhanced.** One switch selects between the original's behaviour, bugs
included, and the version with fixes (`pd_game::settings::Enhancements`). Native:
`--original` / `--enhanced`; web: the "Version" control on the start page. The choice is
remembered in the storage key `PORT.CFG` (`mode=original` or `mode=enhanced`), next to but
not inside `GAME.STP`, which keeps the original's 32-byte format. Without a choice the
remembered one applies; the default is original.

**Game speed.** The original renders as fast as the PC allows and runs
`elapsed 70 Hz ticks + GAME SPEED option + 1` simulation steps per rendered frame, so its
pace depends on the machine. The port renders an island frame every `--frame-ticks` ticks
(default 4, i.e. 17.5 pictures per second; the DOSBox-X reference set-up of
`docs/re/reference.md` runs at 7 to 8). The menu loop, which is unthrottled in the
original too, runs at the rate measured on that reference set-up
(`docs/re/engine/menus.md`).

## Native (Linux)

```sh
cargo build --release -p pd-native
./target/release/pd-native --disc disc/PUNKYDUCK.cue      # or the .bin
./target/release/pd-native --dir disc/extracted/PUNKY     # no CD music
```

Build dependencies inside the box: a C toolchain, `alsa-lib`, `libudev` (gamepad),
Wayland/X11 client libraries. `--no-default-features` drops gamepad support (gilrs).

| Option | Effect |
|---|---|
| `--fullscreen` | start fullscreen (toggle with F11 or Alt+Enter) |
| `--integer` | whole-number scale factors only (toggle with F10) |
| `--no-audio` | do not open an audio device |
| `--trace` | print stage changes, tick and frame rate, audio underruns |
| `--exit-after <s>` | quit after that many seconds |
| `--original`, `--enhanced` | behaviour set (remembered) |
| `--no-intro`, `--no-credits` | skip the intro movies / the credits after EXIT |
| `--frame-ticks <n>` | 70 Hz ticks per rendered island frame (default 4) |
| `--storage <dir>` | directory for `GAME.STP`, `SAVEGAME.REC`, `SAVEGAME.__n`, `PORT.CFG` |
| `--demo` | engine demo instead of the game |
| `--headless` | no window, no audio device: run the game on a simulated 70 Hz clock (see below) |
| `--dump-frames <dir>` | no window: decode every `.MOV`, write first/middle/last frame as PNG (`--dump-every <n>`: every n-th too) |
| `--dump-wav <file>` | no window: run headless on a simulated clock and write the mixed audio (`--seconds <n>`, default 30) plus the final picture as PNG |

With `--enhanced` the island uses modern controls (`Enhancements::modern_controls`): the
mouse turns the duck (the pointer is locked while the island view is up; on the web, click
the view once), W/S walk, A/D strafe, left mouse button or Space fire, number keys 1–9, 0
select weapon 1–10 directly, F3/F4 show the quest pictures; the arrows still turn and all
other keys are as below. The mouse button no longer pauses; P does.

Keys (the original's, `docs/re/engine/input.md`): arrows (or WASD) move, Ctrl strafe,
Space fire, Enter jetpack / confirm, Backspace next weapon, Tab map, Home…Del inventory,
Esc menu, F1/F2 view size / interlace, P pause, 1, 2 quest pictures. In the menus:
Up / Down, Enter, Esc (= last entry), Left / Right for sliders; letters are read as text
(slot names; the level-select phrase has to be typed in capitals, as in the original). Any
key ends the intro movies. The shop's hand is moved with the arrows and "clicks" with
Space. Saved games and `GAME.STP` are interchangeable with the DOS game's files.

### Headless runs

```sh
./target/release/pd-native --disc disc/PUNKYDUCK.cue --headless --no-intro --ticks 700 \
    --script "@100 ArrowDown @140 Enter @200 shot:options" --dump-dir out/shots
```

`--script` tokens: `@<tick>` sets the time in 70 Hz ticks, `<Code>` taps a key for 4 ticks
(`KeyboardEvent.code` names), `+<Code>` / `-<Code>` press / release (island play samples
keys once per rendered frame, so hold them for at least `--frame-ticks` ticks),
`shot:<name>` writes `<dump-dir>/<name>.png`, `do:<say|picture|timed|shop|gameover|
continues|money|ending>` forces a game event for debugging. `--dump-every <n>` writes every
n-th tick, `--frame-log <file>` one line per tick with the CRC-32 (zlib) of the picture as
RGB24, `--dump-wav <file>` the mixed audio. Storage is in memory unless `--storage` is
given. The run prints stage changes, CD commands, the sound effect ids and any missing
files. `out/app-ref/` holds the DOSBox-X side of the comparison (`run.sh` for a private
capture run, `segments.py` to split a capture into still pictures, `compare.py` for
identical-pixel percentage, maximum / mean difference, bounding box and best shift).
 Gamepad: d-pad or left stick, A action, B secondary, X item, Y next item,
LB view, RB inventory, Start cancel, Select pause. Mouse position and both buttons are
reported in 640×480 coordinates.

How it works:

- **Video**: winit window; `pixels` owns the wgpu device, the surface and the 640×480
  RGBA texture; our own pass (`crates/pd-native/src/present.rs`) draws it into the
  largest centred 4:3 rectangle with a nearest-neighbour sampler, black elsewhere.
- **Loop**: a timer wakes the loop at least every 4 ms; each wake advances the app by
  the measured wall-clock time and requests a redraw, which the compositor paces.
  Game time does not depend on the display rate.
- **Audio**: cpal, 44.1 kHz stereo, 1024-frame device buffer. cpal's `pulseaudio`
  feature is enabled because its ALSA host can fail with "device busy" when the
  device is held by a sound server; PipeWire's PulseAudio server takes it. The device
  callback only drains a queue; the main loop keeps about 70 ms queued from
  `Runner::mix_audio` (sound effects plus the CD track from the image). If the main
  loop stalls longer than that, the output has a gap.
- **Storage**: one file per key in `$XDG_DATA_HOME/punkyduck/` (default
  `~/.local/share/punkyduck/`): `GAME.STP`, `SAVEGAME.REC`, `SAVEGAME.__0`..`7` in the
  original's formats, and `PORT.CFG`.

## Web

```sh
web/build.sh                          # one command; add --dev for a debug build
python3 -m http.server -d web 8080    # any static file server works
```

`web/build.sh` installs the `wasm-bindgen-cli` matching
`Cargo.lock` if it is missing, builds `pd-web` for `wasm32-unknown-unknown` and writes
`web/pkg/` (gitignored). Open `http://localhost:8080/`, select or drop your `.bin` and
`.cue`, then click or press a key (audio and the clock start on that gesture). Nothing
is uploaded or fetched: both files are read in the tab and the image stays in wasm
memory (about 127 MB).

- **Video**: a 2D canvas. Each frame the 640×480 RGBA picture is copied out of wasm
  memory with `putImageData`; the canvas element is sized to the largest 4:3 box
  (optionally an integer multiple of 640×480 device pixels) and scaled by the browser
  with `image-rendering: pixelated`. Chosen over WebGL/wgpu because it is much simpler
  (no GPU pipeline, no wgpu in the wasm; the module is about 150 kB) and one 1.2 MB
  copy per frame is negligible.
- **Loop**: `requestAnimationFrame`; elapsed time from its timestamp.
- **Audio**: WebAudio, `AudioContext` at 44.1 kHz created on the start gesture;
  1024-frame chunks from `Player.mix_audio` are scheduled back to back as
  `AudioBufferSourceNode`s about 100 ms ahead. A background tab stops
  `requestAnimationFrame`, so sound pauses there and resynchronises on return.
- **Input**: `KeyboardEvent.code` through the same binding table as native
  (`pd_engine::input::Button::from_key_code`), Gamepad API (standard mapping), pointer
  events. Alt+Enter, the button or a double click toggle fullscreen. Ctrl is the action
  key: Ctrl+arrows work, but shortcuts a page cannot intercept (Ctrl+W, Ctrl+T, Ctrl+N)
  still act, so use the arrow keys rather than WASD in the browser.
- **Text keys**: every key is also passed on as the original's scancode
  (`App::key_event`); the page keeps letters away from the browser while no Ctrl / Alt /
  Meta is held, so slot names and the cheat phrase can be typed.
- **Version**: the "Version" select on the start page (original / enhanced) shows the
  remembered choice and is passed to `new Player(bin, cue, mode, demo)`.
- **Storage**: `localStorage`, keys `punkyduck:<key>`, values hex-encoded (`GAME.STP`,
  `SAVEGAME.REC`, `SAVEGAME.__n`, `PORT.CFG`).

Checks without a visible browser: `node out/app-run/web/wasm_run.mjs <ticks> "<script>"`
runs the wasm module in Node with the headless script syntax and prints the same per-tick
CRC lines as `pd-native --headless --frame-log`, so the two builds can be compared with
`cmp`; `node out/app-run/web/page_test.mjs` drives the real page in headless Chromium
(server on `127.0.0.1:8765`, both stopped at the end) and saves the canvas at the main
menu and the options page.

## Tests

```sh
cargo test --workspace
cargo build -p pd-formats -p pd-engine -p pd-game --target wasm32-unknown-unknown
```

`crates/pd-game/tests/app_flow.rs` drives the whole program flow (menus, `GAME.STP`, cheat,
new game, save and load including a save written by the DOS game, ending, game over and
continue, every modal screen) against `disc/extracted/PUNKY/`; it passes without doing
anything when the files are absent.

`crates/pd-engine/tests/real_disc.rs` runs against `disc/PUNKYDUCK.bin/.cue` and
`disc/extracted/PUNKY/` and prints `SKIPPED` (and passes) when they are absent.

## For game logic

`pd_game::app::Game` implements `pd_engine::App`; the frontends hand it to
`pd_engine::runner::Runner` (`crates/pd-native/src/main.rs`, `crates/pd-web/src/lib.rs`).
Contract additions since the demo (all with defaults, so existing `App`s compile unchanged):

- `App::key_event(scancode, pressed)`: every key as the original's key-table index (set-1
  make code, `+0x80` for `E0` keys); `input::scancode_from_key_code` maps
  `KeyboardEvent.code` names; `Runner::key_event` forwards.
- `App::cd_status(playing)`: called by the runner before each `update`, the original's
  `cd_is_busy` poll. The game restarts the island track itself with it.
- `Command::SetCdVolume { volume }`: the CD volume option (`GAME.STP` value x 16).

Building blocks:

- `runner::FixedStep::original()`: the exact 70 Hz tick (PIT divisor `0x4295`) from the
  variable `dt_us`; `runner::original_substeps(ticks, speed_adjust)`: the original's
  `elapsed + speed_adjust + 1` simulation steps per rendered frame.
- `audio::SfxMixer`: the original mixer (8 channels, 16 kHz signed 16-bit mono, volume
  0..=255, master, divide by `max(active, 4)`, `sfx_play` channel allocation, loop on
  channel 0), resampled to the 44.1 kHz stereo output. `audio::Mixer` is a generic
  alternative with per-voice rate, volume and pan.
- `movie::MoviePlayer`, `assets::{DiscAssets, MemAssets}`, `input::InputState`.
- `Command::PlayCdTrack` / `StopCd` for music. The game plays every track once
  (`looping: false`) and restarts the island track when `cd_status` reports the drive idle,
  as the original does.

## Releases

Pushing a tag that starts with `v` runs `.github/workflows/release.yml`, which tests the
workspace, builds the Linux binary and the web bundle, and publishes both as a GitHub release:

```sh
git tag v1.0.0
git push origin v1.0.0
```

The release carries `punky-duck-<tag>-linux-x86_64.tar.gz` (the `pd-native` binary) and
`punky-duck-<tag>-web.zip` (the static page: unpack it on any static file host).
