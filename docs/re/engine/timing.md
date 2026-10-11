# Frame timing (P.EXE)

All **verified** in disassembly unless marked.

## Clock

- `sound_init` (`0x2f1bc`) ends with `timer_install(0x4295, timer_tick)` (`0x2fa30`):
  hooks **int 1Ch** with the stub `0x2f9d0` and programs PIT channel 0 (mode 3, port
  `0x43` ← `0x36`, port `0x40` ← divisor) with divisor `0x4295` = 17045 →
  1193182 / 17045 = **70.0 Hz**. The stub is non-reentrant (flag `0x41b08`), counts in
  `0x51d0c0` and calls the callback in `0x51d0c4`.
- `timer_tick` (`0x2f832`) does `g_ticks++` (`0x41b02`) and services the sound mixer.
- The timer is only installed when sound is enabled (`0x40004 == 1`, the default in the
  binary). `timer_remove` (`0x2fa90`) restores divisor 0 and re-syncs the DOS clock from
  the CMOS RTC (int 1Ah AH=2 → int 21h AH=2Dh).
- The original BIOS tick handler is not chained at 70 Hz, hence the RTC re-sync.

## Game loop pacing — `logic_substeps` (`0x28d50`)

Called once per rendered frame from `game_tick_frame` (`0x2a260`):

```
input_poll(g_517ec0 == 0);
n = g_ticks;  g_ticks = 0;            // 70 Hz ticks elapsed since last frame
if (!sound_enabled) n = speed_adjust;  // 0x4000c, from GAME.STP[7], default 0
n += speed_adjust;
if (n < 1) { wait_vretrace(); n = 1; }
g_517f70 = n;
for (g_517f44 = 0; g_517f44 <= n; g_517f44++) {   // n + 1 iterations
    demo_step();      // 0x25070
    sub_13480();  sub_14040();
    sub_10860();      // player simulation
    sub_103b0();      // world simulation
}
```

So: **the simulation step is one 70 Hz tick; rendering is uncapped, and each rendered
frame runs `elapsed_ticks + speed_adjust + 1` simulation steps** (note the `<=`: one
step more than the elapsed ticks). If a frame takes less than one tick the loop waits for
vertical retrace and still runs 2 steps. There is no interpolation; per-frame (not
per-tick) work in `game_tick_frame` — `0x11400`, `0x10290`, `0x11be0`, timers
`0x517eec`/`0x517f24` — runs once per rendered frame, so those parts are frame-rate
dependent in the original. `g_517f70` (steps this frame) is available to gameplay code.

For the port: run the tick at 70 Hz; to reproduce the original feel, group ticks per
presented frame as above (hypothesis: on period hardware the game rendered ~15–25 fps,
i.e. 3–5 ticks per frame; measure with the DOSBox reference if exact pacing matters).

## Other waits

- `palette_fade` (`0x331f0`): `steps + 1` iterations, each preceded by `wait_vretrace`
  (`0x30ddf`, port `0x3DA` bit 3). Fades are therefore timed by the display refresh
  (70 Hz in VESA mode 0x101 / mode 13h).
- `flc_play` (`0x2bc70`): waits `N` vertical retraces per frame, `N` computed from the
  FLC header speed field (float math through `__CHP`; exact formula **hypothesis**:
  speed ms → retraces at 70 Hz).
- Message box flash `0x2a320`: `wait_hretrace(4000)` (`0x30df2`) between blits.
- Idle/demo: `input_poll` sets the "space" input after `g_ticks > 0x118` (280 ticks =
  4 s) with no key, while `0x41628 == 0` (attract mode start, **hypothesis** on purpose);
  `0x50d5d4` accumulates idle ticks.
- Menu code resets `g_ticks = 0` at page changes and uses it for its own timeouts
  (`0x27818`: 0x118 ticks).
