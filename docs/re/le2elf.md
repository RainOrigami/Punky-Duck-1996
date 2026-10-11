# le2elf — LE executables as ELF32

`tools/le2elf` converts the DOS/4GW LE executables into static ELF32 i386 files that
ordinary tools load. Addresses in the ELF are the LE *relocation base* addresses the
linker assigned, with all fixups already applied, so every absolute pointer in code and
data is directly readable.

## Produce

```
cargo build --release -p le2elf
for n in P INTRO THE_END; do
  target/release/le2elf disc/extracted/PUNKY/$n.EXE disc/extracted/$n.elf \
      --fixups disc/extracted/$n.fixups.txt
done
```

Outputs (gitignored, under `disc/extracted/`): `P.elf`, `INTRO.elf`, `THE_END.elf`, and
`*.fixups.txt` (one line per fixup: `<address of patched dword> <pointer value>`, sorted).
The fixup list is a complete list of every absolute pointer in the image — useful for
telling pointers from integers in data tables and for finding jump tables.

## Which tools open it — **verified**

- Ghidra GUI: import `disc/extracted/P.elf` as ELF
  (Ghidra 12.1.2 provider, x86 32-bit; default analysis finds 538 procedures, 348 strings).
- Ghidra headless: `support/analyzeHeadless` of the Ghidra installation (ELF loader, `x86:LE:32:default`).
- `objdump -d -M intel P.elf`, `readelf -lS P.elf`.

File offset of an address: object 1 `addr - 0x10000 + 0x1000`; see `readelf -l`.

## Address layout of P.elf — **verified** (LE object table at file `0x2d44`)

| LE obj | ELF sections | Address range | Flags | Content |
|---|---|---|---|---|
| 1 | `.text1` | `0x10000`–`0x3df0f` | R-X | all code (game + Watcom runtime), plus const data/jump tables embedded in code |
| 2 | `.data2` + `.bss2` | `0x40000`–`0x520bc0` | RW- | initialised data `0x40000`–`0x44000` (strings, tables); rest is zero BSS incl. the stack at the top |
| 3 | `.data3` + `.bss3` | `0x530000`–`0x5337ec` | RW- | separate data object (2 file pages) |
| 4 | `.data4` + `.bss4` | `0x540000`–`0x540808` | RW- | separate data object (1 file page) |

- Entry `0x33db8` (ELF symbol `_start`): `jmp` over the string
  `WATCOM C/C++32 Run-Time system. (c) Copyright by WATCOM International Corp. 1988-1995`,
  then `mov ah,30h / int 21h`, DOS/4G signature check (`cmp ax,'DX'`). **verified**
- Initial ESP `0x520bc0` = end of object 2 (ELF symbol `__le_stack_top`). **verified**
- INTRO.elf: objects at `0x10000`, `0x20000`, `0x80000`, `0x90000`, entry `0x1589c`.
- THE_END.elf: objects at `0x10000`, `0x20000`, `0x110000`, `0x120000`, entry `0x158f0`.

## LE facts the converter relies on — **verified** against the files

Structure layout per Open Watcom `bld/watcom/h/exeflat.h` (`os2_flat_header`,
`object_record`, `le_map_entry`, `OSF_*`), checked against the three executables:

- MZ stub `e_lfanew` (0x3c) = `0x2c80` points straight at `LE\0\0` (the DOS/4GW loader
  is not bound in; `DOS4GW.EXE` ships separately). Byte/word order 0, format level 0.
- Header: `num_pages` +0x14, entry obj/EIP +0x18/+0x1c, stack obj/ESP +0x20/+0x24,
  page size +0x28 (4096), **last page size** +0x2c (`0x7fe`), object table +0x40/count +0x44,
  page map +0x48, fixup page table +0x68, fixup record table +0x6c (all relative to the LE
  header), **data pages offset +0x80 relative to the start of the file**. For all three
  files `data_pages + (num_pages-1)*4096 + last_page_size` equals the file size exactly.
- Object table entry (24 bytes): vsize, base, flags, first page-map index (1-based),
  page count, reserved. Flags `0x2045` = R+X+preload+32-bit, `0x2043` = R+W+preload+32-bit.
- Page map entry (4 bytes): 3-byte big-endian physical page number, 1 flag byte. All
  entries are identity-mapped with flag 0. No iterated/compressed pages (`idmap_off` = 0).
- Fixup page table: `num_pages + 1` u32 offsets into the record table; the last entry
  equals the record table length (P.EXE `0x16217`).
- Fixup records: only **source type 7 (32-bit offset), internal target**, with target
  flags `0x00` (16-bit target offset) or `0x10` (32-bit target offset). Record =
  `src u8, flags u8, srcoff i16, object u8, target offset u16|u32`. Value written =
  `base[object] + offset`. A fixup that straddles a page boundary appears in both pages,
  with a negative `srcoff` in the second; it is applied once.

| File | Records | Applied | Cross-page duplicates | flags 0x00 / 0x10 |
|---|---|---|---|---|
| P.EXE | 10655 | 10647 | 8 | 2624 / 8031 |
| INTRO.EXE | 1758 | 1756 | 2 | 1150 / 608 |
| THE_END.EXE | 1218 | 1218 | 0 | 619 / 599 |

Every record is parsed and the parser must land exactly on each page's end offset, or
the tool aborts; so "records parsed" = "records in the table".

Validation done: string references resolve (`push 0x42fd8` at `0x2bc28` → `"BANQUE.BNK"`;
the dword at `0x2b600` → `"PICTURE.BNK"` at `0x42fc4`), entry is Watcom startup code.

## Caveats

- No symbols exist in the originals; the ELF has only `_start` and `__le_stack_top`.
  Names come from `functions.md`.
- Implemented but never exercised by these files: source-list records (`0x20`),
  additive values, 16-bit object numbers, source types 5 (16-bit offset) and 8
  (self-relative). Anything else (selector / far-pointer sources, imports, entry-table
  targets, alias, chained fixups, iterated pages) makes the tool exit with an
  `UNSUPPORTED FIXUP` error rather than skipping.
- Real DOS/4GW loads objects wherever DPMI gives it memory; the ELF addresses are
  link-time addresses. Segment registers: code reads `es:`/`fs:` with flat selectors in
  places (e.g. PSP access at startup); disassemblers show these as plain memory operands.
- The BSS of object 2 is 5 MB; Ghidra allocates it as uninitialised memory.
- Calling convention seen so far is stack-based (args pushed, caller cleans up:
  `push 0x42fd8; call 0x2b4e0; add esp,4` at `0x2bc28`), i.e. compiled with Watcom
  `-3s`/`-4s`/`-5s`; expect Watcom's register convention only in hand-written asm and
  possibly in runtime internals. **verified** for that call site, hypothesis in general.
