# Sound (P.EXE)

**Verified** in disassembly unless marked.

## Driver layer

Digital output goes through an external real-mode driver: `SOUNDRV.COM` is loaded into
DOS memory (`0x2ebb6`) and called through int 66h thunks (`0x18a78`–`0x18dec`);
`VECTOR.COM` (`0x2edb6`) installs that vector. P.EXE only asks the driver for its
capability word (`0x18b45`: bit `0x800` 16-bit stereo, `0x400` 16-bit, `0x80` 8-bit,
`0x200` DMA available), hands it buffers and polls its state. None of this is needed for
the port; what matters is the mixer below.

## Mixer

- Output rate **16000 Hz** (hard-coded, printed as `Freq: 16000`), buffer **630 sample
  frames** (`0x276`, `Swid`), double-buffered (two halves at `0x51d0b0`).
- `timer_tick` (`0x2f832`, 70 Hz): if `sound_ready` (`0x41afe`) and `mixer_on`
  (`0x51c2d8`) and the driver reports the playing half finished (`0x2f76b` ≠ 2), call the
  mix routine in `0x51c4f0` on the idle half and swap.
- **8 channels**, 64 bytes each at `0x51c2f0 + ch*0x40`:
  `+0 active`, `+4 cur ptr`, `+8 volume 0..255`, `+0xc desc[0]`, `+0x10 start`,
  `+0x14 loop start`, `+0x18 loop end`, `+0x1c end ptr`, `+0x20 loop count`.
  Active count `0x51c2ec`, master volume `0x51c2e8` (= `GAME.STP` volume × 16).
- Per output sample (`mix_16m` `0x2e6d0`, `mix_8m` `0x2e4d0` identical up to the store):

  ```
  acc = 0
  for ch in 0..8: if active:
      acc += (i16 at cur) * volume >> 8;  cur += 2
      if cur >= end && loop_count == 0: active = 0; nactive--
      if cur == loop_end && loop_count != 0: loop_count--; cur = loop_start
  acc = acc * master >> 8
  acc = nactive > 4 ? acc / nactive : acc / 4        // (signed divide; the /4 rounds toward 0)
  16-bit: out = (i16)acc          8-bit: out = (u8)((acc ^ 0x8000) >> 8)
  ```

  **So samples are signed 16-bit little-endian mono at 16000 Hz, played without
  resampling or pitch control.** Note `nactive` is re-read after the channel loop, and a
  loop count of −1 (`0xffffffff`) decrements forever → effectively infinite.

## Sample API

- 29 samples, names in the pointer table at `0x41a84`, index = sample id:
  0 bonus11, 1 boom11, 2 duck11, 3 mer11, 4 monste11, 5 pack11, 6 zap11, 7 flopg-11,
  8 flopd-11, 9 pas-g-11, 10 pas-d-11, 11 move11, 12 yes11, 13 lance11, 14 dollar,
  15 gat11, 16 impact11, 17 mdead1, 18 mdead2, 19 mtouch1, 20 mtouch2, 21 punch11,
  22 teleport, 23 weapon11, 24 zop11, 25 pose, 26 take, 27 good, 28 rire (`.pcm`).
- Descriptor, 42 bytes at `0x51cbdc + id*0x2a`: `+0 = 1`, `+0x12 data ptr`,
  `+0x16 length`, `+0x1a loop start offset (0)`, `+0x1e loop length (= length)`,
  `+0x22 = 0`. Lengths are in bytes.
- `sfx_play(id, vol)` (`0x2ef10`, 136 call sites): channel = first inactive of 1..7,
  else the channel 1..7 with the fewest samples left (`0x2eeb0`); starts with loop count 0.
- `sfx_play_loop(id, vol)` (`0x2ef40`): channel 0, loop count −1. Used once:
  `mer11.pcm` (sea) at island start with volume 0; `channel0_set_volume` (`0x2ef70`,
  5 call sites) fades it by distance/condition (gameplay).

## CD audio

MSCDEX through int 2Fh AX=1510h (device request) with request headers in DOS memory:
`cd_play_track(n)` (`0x1e740`) reads the start of track `n` and `n+1` (IOCTL input 0Bh,
Red Book MSF → frames via `0x1e2f0`) and issues PLAY AUDIO for the difference;
`cd_stop` (`0x1e860`) issues STOP twice; `cd_is_busy` (`0x1e880`) tests the busy bit;
`cd_set_volume` (`0x1e950`) sets all four channel volumes to `GAME.STP` CD volume × 16.
Music does not loop by itself: `cd_loop_check` (`0x11f70`) restarts track 5 every 10th
frame if the drive is idle. Track usage table: `states.md`.
