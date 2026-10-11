# `ILE0n.TRJ`, `MONSTERn.TRJ` — placed bonuses and monsters

Parser: `crates/pd-formats/src/trj.rs`. Dump: `ile0n_trj.json`, `monstern_trj.json`,
markers on `ile_n_overlay.png`.

Both families share one layout. Despite the extension, `ILE0n.TRJ` is the **bonus (pickup)
list** of an island and `MONSTERn.TRJ` the **monster list**; only monsters use the path part.

## Layout — **verified**

```
u32 path_count
path_count   x { u32 first_point, u32 point_count }     runs inside the point array
u32 record_count
record_count x 32-byte record
u32 point_count
point_count  x { f32 x, f32 z }                         path-local coordinates
```

Sizes add up exactly for all ten files. **verified** (unit tests).

Record:

| Offset | Type | Field | Used by |
|---|---|---|---|
| 0 | i32 | 32 in every record | nobody |
| 4 | i32 | path index | monsters (`0x243d4`) |
| 8 | i32 | 0 in every record | nobody |
| 12 | i32 | world X | both |
| 16 | i32 | world Z | both |
| 20 | f32 | path scale | monsters (`0x24489`) |
| 24 | f32 | path angle, radians | monsters (`0x243f6`: cos/sin) |
| 28 | i32 | kind | both |

World X/Z are the terrain's world units (300 per cell, origin at map centre; see `uni.md`).
**verified** by output: drawn with `cell = (w + 38400) / 300`, all bonuses and monsters of
all five islands sit on land and bonuses line pond shores.
The island-1 start area (player start constants -19650/-10575 at `0x17712`, sign flipped)
has two monsters, a bonus and brown rocky ground, consistent with a DOSBox capture of the
first seconds of play; that match is by eye only (**hypothesis** for the sign flip).

## `ILE0n.TRJ` — loader `0x27fd0` — **verified**

- Reads `path_count`, then `path_count * 8` bytes into a scratch buffer (ignored).
- Reads `record_count`; `>= 1600` aborts with "Erreur nombre de bonus".
- For each record calls `0x27f30(index, kind, x, -600, z)`; `kind` outside 0..54 prints
  "Erreur BONUS". Y is the constant -600. Path, scale and angle are ignored
  (on disc: path 0, scale 1.0, angle 0 in every record).
- The point block (22 points on every island: paths `(0,1) (0,16) (16,6)`) is never read.
- Further bonuses are added at random free cells by `0x28bc0` after loading.

Kinds on disc: 0..45. Kinds >= 24 are the mass pickups (hundreds per island), kinds < 24
occur once or a few times (keys/special items — **hypothesis**; the bonus sprite table will
tell).

| Island | bonuses |
|---|---|
| 1 | 1015 |
| 2 | 773 |
| 3 | 533 |
| 4 | 275 |
| 5 | 304 |

## `MONSTERn.TRJ` — loader `0x24000` — **verified**

- File name is built from `Monster1.Trj` with the island digit patched (`0x24015`).
- Up to `0x2800` bytes are read in one go to `0x4f6144`; pointers are then set:
  paths `0x414b8`, records `0x414b4`, points `0x414bc`, `record_count` `0x414a8`.
- `record_count >= 210` aborts with "Internal Error : BAD TRJ (%d) !".
- Spawn (`0x24110`, one 388-byte monster struct at `0x4f8944 + i*0x184` per record):
  monster type = `kind + 1`; `0x4159b[type]` selects the behaviour class; several stats are
  scaled by the difficulty setting (`0x41624`).

Counts: 147 / 160 / 199 / 199 / 200 monsters; kinds 0..2 on island 1, 0..3 elsewhere.
Path 0 and 1 are single points `(0,0)` (stationary spawn); paths 2+ have 3-6 points.

### Path arithmetic — **verified** as code, behaviour in play **not confirmed**

Float to int is truncation (`0x305a4` forces RC=chop).

Spawn (`0x2440c`): `P0` = first path point, `pivot = trunc(P0)`.
If the path has more than one point, `target = W(pivot, P1)`, else `target = pivot`.
Then `record.x/z` is added to both; `pivot` becomes the monster position.

```
W(c, P):  tx = trunc(((P.z - c.z) * sin a + (P.x - c.x) * cos a) * scale + c.x)
          tz = trunc(((P.z - c.z) * cos a - (P.x - c.x) * sin a) * scale + c.z)
```

Velocity = `speed * (target - pos) / distance` per axis (`0x245af`). A waypoint counts as
reached when `|pos.x - target.x| < vx` and `|pos.z - target.z| < vz` (`0x22fbe`; signed
compare against the velocity component). Then (`0x23030`) the position snaps to the target,
the point index advances (wrapping to 0 at `point_count`) and the next target is
`W(position, P[index])` — with `position` in **world** coordinates and `P` still path-local,
and without adding `record.x/z` again.

Taken literally this does not trace the small path shape around the spawn point: after the
first leg the `- position` term dominates and the target lies towards the map origin (or, for
angles near -pi/2, sideways around it). `pd-extract` writes these computed points as
`lap_as_coded` in the JSON but does not draw them. Open question for the gameplay port: check
a path monster (e.g. island 1 record at x=17925, z=11250, kind 1, path 2) in DOSBox to see
whether it really leaves towards the centre, is stopped by terrain attributes, or whether
another code path governs these monsters. The struct fields `+0x154..+0x180` are reused with
other meanings by other behaviour branches (`0x22f53`), so the class matters.
