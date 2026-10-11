# `.BNK` archives

Code: `crates/pd-formats/src/bnk.rs`. Addresses are `P.EXE` linear addresses (code object at `0x10000`).

| File | Members | Opened by |
|---|---|---|
| `BANQUE.BNK` | 30 × `.SPT` | `0x2bc20` (reads a whole member into a caller buffer) |
| `PICTURE.BNK` | 54 × `.RON` | `0x2b5f0` (decodes a member, see `ron.md`) |
| `SAMPLE.BNK` | 29 × `.PCM` | `0x2ef80` (path `c:\PUNKY\SAMPLE.BNK`, first letter replaced by `'A' + byte[0x1c6ee4]`) |

## Layout — **verified** (`0x2b4f0`)

```
u32 count
count × { char name[12]; u32 offset; u32 size; }   // 20 bytes each
member data
```

- `name` is NUL-padded; a 12-character name (`TRANS1-2.RON`) has no terminator.
- `offset` is absolute from the start of the file.
- All integers little-endian.

## Lookup — **verified** (`0x2b4f0`, compare at `0x35b18`)

Linear scan in directory order; the first record whose name matches wins. The compare is
`strnicmp(record, wanted, 12)`: only `A`–`Z` are folded, at most 12 bytes are compared. The
game asks for mixed-case names (`Ciel_1.RON`, `coolfnt1.spt`); the directory holds upper case.
The archive is reopened and rescanned for every request.

## Observations on the shipped files — **verified** (unit test `disc_archives`)

- Members are stored back to back in directory order, starting right after the directory,
  and the last one ends at the end of the file. No padding, no checksums.
- `PICTURE.BNK` contains `MENUSPIC.RON`, byte-identical to the loose file. `BANQUE.BNK`
  contains `COOLFNT1..3.SPT`, byte-identical to the loose files. The game reads the archive
  copies; `INSTALL.RON` and `INSTFNT1..3.SPT` exist only loose and belong to `INST.EXE`.
- `BANQUE.BNK` members `COOLFONT.SPT`, `BOOM_1.SPT` are not named anywhere in `P.EXE`
  (`BOOM_1.SPT` is byte-identical to `BOOM.SPT`) — **hypothesis**: unused leftovers.
