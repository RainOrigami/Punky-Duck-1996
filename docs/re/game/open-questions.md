# Open questions (things to observe in the running original)

Method that works: `tools/dosbox/run.sh` with AUTOTYPE; a save game written by the original
is a complete state dump (`save-and-settings.md` 2) — edit it, load it, play, save again,
diff. Reaching the save: in play `esc`, then `down down down enter enter enter enter`
(one Esc only: a second Esc selects EXIT). Loading by script works with
`down enter enter down enter` from the main menu **when `SAVEGAME.REC` is present next to
the save** (the failed first attempt had only `SAVEGAME.__0`). Full recipe, including
per-step input recording and a deterministic step schedule: `reference.md`,
"Deterministic state comparison".

Resolved since the first round: Q2 (`picture_timed`: argument = fade steps, hold =
`delay(100)`), Q7 (path monsters do drift, observed), Q9 (no soft lock), Q12 (animation and
facing read in assembly, `entities.md` 3.5), Q13 (`explored` writer `0x12aff`), Q14 (hurt is
20 on all island-4 pads), Q15 (the HUD double life loss happens in the original).

| # | Question | Why it matters | Concrete check |
|---|---|---|---|
| Q1 | Is the island-1 → ending jump (`0x124ea`) a preview cut? | port's default flow | Strong static evidence that it is a cut (`overview.md`). To close: find another release of P.EXE and compare the bytes at `0x124ea` (file offset `0x19200 + 0x24ea`); or finish island 1 in DOSBox and confirm PART_1 plays. |
| Q3 | Real ticks per frame on period hardware / in DOSBox at 30000 cycles (at 150000 cycles about 40 frames per second were seen in recordings) | feel of movement vs. monsters (per-step vs per-frame split, `player.md` 3.1) | count rendered frames per second in a capture while walking (new frames differ; refresh is 70 Hz) → `ticks = 70/fps` |
| Q4 | Sample index → file mapping | sounds | configure a DIGPAK driver (reference.md says sound is off) and capture audio for pickup (0), money (14), teleport (22) |
| Q5 | Does attract/demo mode ever start without `DEMOILEn.DAT`? | menu idle behaviour | leave the main menu idle for 2 minutes in a capture |
| Q6 | Purpose of the DUCK frame 0x2d queued 3000 units ahead at scale −999 (`0x11441`) | renderer contract (probably a camera/sort anchor, never visible) | `engine/renderer.md` on negative scale; or capture and look for a duck sprite ahead of the player |
| Q8 | Zone numbers → what they look like per island | level art understanding only; behaviour is in `player.md` | overlay `out/world/ile_n_attr.png` on the island render |
| Q17 | The stale-register store of the weapon-7 cluster (`player.md` 5) lands on an unknown address | weapon 7 only | read `main`'s EDI at the call, or record a run firing weapon 7 from an edited save |
