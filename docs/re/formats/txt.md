# `ILE_n.TXT` dialogue and the text encoding

Code: `crates/pd-formats/src/txt.rs`. Addresses are `P.EXE` linear addresses.

## File structure — **verified** (`0x25140`)

Plain text, CRLF line ends. Repeated blocks of six lines:

```
<id>
"fr 1","fr 2","fr 3","fr 4"
"en 1","en 2","en 3","en 4"
"es 1","es 2","es 3","es 4"
"de 1","de 2","de 3","de 4"
"pl 1","pl 2","pl 3","pl 4"
```

How the game reads it (`get_text(id)`, `0x25140`):

1. `sprintf(key, "%d", id)`; open `ILE_<island>.TXT` (digit patched into `ILE_0.TXT` at
   `0x416ec`), mode `"rb"`.
2. `fgets` (500 bytes) until `strstr(line, key) == line`, i.e. the first line that **starts
   with** the id. Ids ascend in every file, so this behaves as an exact match. A missing id
   never returns (the loop has no end-of-file check).
3. Read `language + 1` more lines (`language` = dword `0x416e4`, 0 = FR … 4 = PL) and keep
   the last.
4. Four times: skip to the next `"`, copy bytes up to the following `"` into one of four
   256-byte buffers (`0x50d1c4`, `0x50d2c4`, `0x50d3c4`, `0x50d4c4`). Whatever sits between
   the strings is ignored, so the stray trailing comma in `ILE_2.TXT` id 2 is harmless.
   There is no escape character.

The four strings are the display lines of one speech bubble; unused ones are empty.

## Shipped files — **verified** (unit test `disc_texts`)

| File | Entries | Ids |
|---|---|---|
| `ILE_1.TXT` | 19 | 0–18 |
| `ILE_2.TXT` | 37 | 0–36 |
| `ILE_3.TXT` | 42 | 0–41 |
| `ILE_4.TXT` | 40 | 0–39 |
| `ILE_5.TXT` | 22 | 0–9, 12–23 (10 and 11 are absent) |

`ILE_1.TXT` id 18 has only four language lines (all empty) and then the file ends. In the
game the failed `fgets` leaves the previous line in the buffer, so Polish gets the German
line again; `TextFile::parse` reproduces that. `ILE_2.TXT` and `ILE_3.TXT` end with an extra
blank line.

## Encoding — **verified** against the font

Text bytes index the font directly (glyph = byte − `0x20`, see `spt.md`), so the glyph
images of `COOLFNT1.SPT` define the character set. There is one character set for all five
languages; the Polish lines simply use a block the others do not.

| Bytes | Meaning |
|---|---|
| `0x20`–`0x7E` | ASCII. Glyphs `0x2A` and `0x2B` are drawn as vertical bars, not `*` and `+` |
| `0x80`–`0xAF` | **Code page 437** (`ü é â ä à ç ê è î ô ö û ù Ü á í ó ú ñ ¿ ¡` are the ones used). Glyphs exist for `0x80`–`0x9C`, `0xA0`–`0xA8`, `0xAD`–`0xAF`; `0x9D`–`0x9F` and `0xA9`–`0xAC` are empty |
| `0xB0`–`0xD0` | Polish alphabet, lower case, in this order: `a ą b c ć d e ę f g h i j k l ł m n ń o p r s ś t u ó w z ż ź x y` |
| `0xD1`–`0xF1` | the same 33 letters in upper case: `A Ą B C Ć D E Ę F G H I J K L Ł M N Ń O P R S Ś T U Ó W Z Ż Ź X Y` |
| other | no glyph |

The `0xB0`–`0xF1` block is the game's own; it is not CP852, Mazovia, ISO 8859-2 or
Windows-1250 (e.g. `ę` is `0xB7` here, `0xA9` / `0x91` / `0xEA` / `0xEA` there). Because it
overlays the CP437 box-drawing and Greek range, `ß` (`0xE1` in CP437) has no glyph; the
German text avoids it (`muss`).

Bytes used by the shipped texts, per language:

| Language | High bytes |
|---|---|
| FR | `82 83 85 87 88 8A 8C 93 96 97` |
| EN | `82` |
| ES | `82 A0 A1 A2 A3 A4 A8 AD` |
| DE | `81 83 84 94 9A` |
| PL | `A2` (ó, from the CP437 range) and `B1 B4 B7 BF C2 C7 CD CE E8 EF` (ą ć ę ł ń ś ż ź Ś Ź) |

The Polish reading was cross-checked on words: `wezmę`, `użyteczny`, `śliczne rączki`,
`pudełko`, `znaleźć`, `gwoździe`, `końca`, `potworów`, `Świetnie`, `Źle`. `0xCD` (dot) vs
`0xCE` (acute) and `0xEE` vs `0xEF` were told apart on enlarged glyphs.

`txt::decode` maps to UTF-8, `txt::encode_char` goes back (lowest byte wins, so `ó` encodes
as `0xA2` and plain letters stay ASCII).
