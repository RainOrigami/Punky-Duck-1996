# `.PCM` sound effects

Code: `crates/pd-formats/src/pcm.rs`. Addresses are `P.EXE` linear addresses.

**Headerless signed 16-bit little-endian PCM, mono, 16000 Hz.**

## Evidence — **verified**

- Loader `0x2ef80`: opens the member in `SAMPLE.BNK`, `malloc(size)`, locks the memory
  (`0x2f100`), `fread`s the whole member. No header is parsed. `0x2f020` does this for the
  29 names in the pointer table at `0x41a84` and fills 42-byte sample descriptors at
  `0x51cbdc` (data pointer at +0x12, length in bytes at +0x16, loop start +0x1a, loop
  length +0x1e; loop fields are set to 0 / full length).
- Mixer `0x2e4d0` (8-bit mono output; `0x2e5d0`, `0x2e6d0`, `0x2e7d0` are the 8-bit stereo,
  16-bit mono and 16-bit stereo siblings): for each of 8 voices, `movsx edi, word [esi]`,
  multiply by the voice volume (0..255), `sar 8`, `add esi, 2`. One source sample per output
  frame: **signed 16-bit, mono, no resampling**, so the file rate equals the output rate.
  The sum is scaled by the master volume / 256 and divided by 4, or by the number of active
  voices when more than 4 are playing. A voice ends when its pointer reaches the end
  (`(end - cur) >> 1` samples remain, `0x2eed0`).
- Output rate: every mode sets the DIGPAK sound structure's frequency word to `0x3e80` =
  16000 (`mov word [eax+0xa], 0x3e80` at `0x2f37e`, `0x2f3f7`, `0x2f463`, `0x2f5b4`, `0x2f659`,
  `0x2f6f6`), and init prints `Freq: %d` with 16000 and `Swid: %d` with 630 (`0x2f2d8`),
  630 being the mixed block length in frames.

The `11` in names such as `DUCK11.PCM` is not the sample rate — **hypothesis**: a left-over
of the source material's naming (11 kHz originals resampled for the game).

Not checked by ear against the original: the shipped `SOUNDRV.COM` is the DIGPAK "no sound"
driver, so the reference DOSBox setup is silent until a real driver is configured
(see `reference.md`).

## Files — **verified** (unit test `disc_samples`)

29 members, 1530 to 200180 bytes (0.05 s to 6.3 s). All have even length.

`PAS-G-11.PCM` and `PAS-D-11.PCM` (left / right footstep) are 10-byte stubs: five zero
samples. They are loaded and played like any other effect and are inaudible.
`Pcm::is_silent()` flags them.
