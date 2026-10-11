<div align="center">

# Punky Duck (1996), rebuilt

### [▶ Play in your browser](https://punky-duck.staticdomains.app/)

**https://punky-duck.staticdomains.app/**

A from-scratch engine for Condor Software's 1996 DOS game *Punky Duck*, written in Rust.
Runs natively on Linux and in the browser, using the data from the original CD.

</div>

---

## Playing

The game data is not part of this project. You need the original CD image as a `.bin` and
`.cue` pair. An image is archived at the Internet Archive:
[archive.org/details/punky-duck](https://archive.org/details/punky-duck).

**In the browser:** open the [play page](https://punky-duck.staticdomains.app/), drop the
`.bin` and `.cue` on it, and start. The files are read in your browser tab and are not
uploaded anywhere. The page has a Help button listing every control, option and cheat.

**On Linux:** download the binary from the
[latest release](https://github.com/RainOrigami/Punky-Duck-1996/releases/latest), or build it:

```sh
cargo build --release -p pd-native
./target/release/pd-native --disc path/to/PUNKYDUCK.cue             # original behaviour
./target/release/pd-native --disc path/to/PUNKYDUCK.cue --enhanced  # modern controls and fixes
./target/release/pd-native --disc path/to/PUNKYDUCK.cue --enhanced --without minimap,far_view
```

More options, the web build and headless runs are in [docs/build.md](docs/build.md).

## Original and enhanced

One switch selects how the game behaves.

- **Original** reproduces the DOS release, bugs included. As on the original disc, finishing
  the first island plays the ending; the other four islands are reached through the
  original's level-select cheat or a saved game.
- **Enhanced** modernises it:
  - *Controls:* mouse look with adjustable speed, WASD with strafing (over water too), fire
    on the left mouse button, number keys for weapons, E for the jetpack.
  - *Picture:* the island view is drawn on every screen refresh instead of about 17 times a
    second, reaches four times as far, and has a minimap.
  - *Menus:* mouse, WASD and Space work in the menus and the shop; Esc in the main menu
    returns to the game; pictures and dialogue wait for Space, Enter or a click instead of
    vanishing on the next movement key.
  - *Game:* finishing island 1 leads on to island 2, the compass survives loading, and
    monsters leave you alone for the first seconds after you appear.
  - *Bug fixes:* monsters keep to their patrol paths, a death costs one life instead of
    sometimes two, the pickups the original loses at island start are there, and more.

Each enhancement can be switched off on its own: on the play page under *Choose
enhancements*, natively with `--without <names>` (`--list-enhancements` prints the names).
All of them are named flags in
[`crates/pd-game/src/settings.rs`](crates/pd-game/src/settings.rs), each documented with the
original behaviour it replaces.

### Controls on the islands

| Action | Original | Enhanced |
|---|---|---|
| Walk | Up / Down, or W / S | W / S, or Up / Down |
| Turn | Left / Right, or A / D | Mouse, or Left / Right |
| Strafe | Hold Ctrl and turn | A / D |
| Fire | Space | Left mouse button, or Space |
| Choose weapon | Backspace (next) | 1–9, 0 directly; Backspace (next) |
| Jetpack | Enter | E, or Enter |
| Map | Tab | Tab |
| Inventory | Home, End, PgUp, PgDn, Ins or Del | same |
| Quest pictures | 1, 2 | F3, F4 |
| Pause | P, or a mouse click | P |
| Close a picture or dialogue | Any game key | Space, Enter, Tab or a click |
| Menu (save, load, options) | Esc | Esc (and Esc again to return) |

In the enhanced version the menus and the shop also take the mouse, and the mouse speed is
set under OPTIONS, CONTROLLER OPTIONS.

### Level select

Type `THE TRUTH IS OUT THERE` in capitals in any menu. The screen flashes red, and NEW GAME
then lets you pick the island. This is the original game's own cheat. The in-game cheats are
listed on the play page's Help panel.

## State of the port

The whole game is playable from the intro to the credits: all five islands with their quests,
shops and bosses, sound effects and CD music, saving and loading. Settings and saved games
are interchangeable with the DOS game's files.

To keep it faithful, the port was compared with the original running in DOSBox-X: menus and
the island view picture for picture, the game rules through recorded runs that end in
identical save files.

Still open:

- The game itself still runs at the original's pace of about 17 steps a second; the enhanced
  version draws the view in between, moving monsters and shots between their two known
  positions.
- With the far view, monsters only wake up at the original distance, closer than the
  scenery now reaches.
- Gamepads are untested.
- Attract mode and joystick calibration are not implemented.

## How it was made

The original is a Watcom C, DOS/4GW program with no symbols. The work went in this order:

1. [`tools/le2elf`](tools/le2elf) converts the LE executable into an ELF file that current
   disassemblers open.
2. The file formats and the engine were read from the disassembly and written up in
   [`docs/re/`](docs/re/README.md): archives, pictures (GIF files in disguise), sprites,
   terrain, meshes, movies, the renderer, timing, sound, input, and the game rules.
3. Each part was reimplemented in Rust and compared with the original running in DOSBox-X
   ([`docs/re/reference.md`](docs/re/reference.md)).

## Layout

| Path | Contents |
|---|---|
| `crates/pd-formats` | Decoders for the original file formats and the CD image |
| `crates/pd-engine` | Platform contract, 70 Hz runner, mixer, movie player |
| `crates/pd-game` | Renderer, simulation, menus and program flow |
| `crates/pd-native` | Linux frontend (winit, wgpu, cpal) |
| `crates/pd-web`, `web/` | Browser frontend (WebAssembly) and its page |
| `crates/pd-extract` | Dumps the original data to PNG, WAV, OBJ and JSON |
| `tools/le2elf` | LE to ELF converter |
| `tools/dosbox` | Reference set-up for running the original in DOSBox-X |
| `docs/re` | Reverse-engineering notes |

## Credits

Original game by Condor Software, 1996. Reverse engineering and the Rust port were done with
[Claude](https://claude.com/claude-code).

## Licence and legal

The code and documentation in this repository are released into the public domain under
[The Unlicense](LICENSE).

This repository contains no data from the original game. *Punky Duck* is © 1996 Condor
Software. This is an unofficial fan project and is not affiliated with the original authors.
