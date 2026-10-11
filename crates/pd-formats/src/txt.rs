//! `ILE_n.TXT` dialogue files and the game's text encoding.
//!
//! Spec: `docs/re/formats/txt.md`. Follows the lookup routine at
//! `P.EXE` `0x25140`; the character set is read off the glyphs of
//! `COOLFNT1.SPT` (glyph index = byte - 0x20, see `spt::Font`).

use std::fmt;

/// Languages in file order; the game reads `language + 1` lines after the
/// id line and uses the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    French = 0,
    English = 1,
    Spanish = 2,
    German = 3,
    Polish = 4,
}

impl Language {
    pub const ALL: [Language; 5] =
        [Language::French, Language::English, Language::Spanish, Language::German, Language::Polish];

    /// Two-letter code, for file names and JSON keys.
    pub fn code(self) -> &'static str {
        ["fr", "en", "es", "de", "pl"][self as usize]
    }
}

/// Strings per language line.
pub const STRINGS_PER_LINE: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxtError(pub String);

impl fmt::Display for TxtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TXT: {}", self.0)
    }
}

impl std::error::Error for TxtError {}

/// One dialogue: an id and, per language, four display lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntry {
    pub id: u32,
    /// The id line as written (without line ending); the game matches on
    /// this by prefix, see [`TextFile::lookup`].
    pub id_line: Vec<u8>,
    /// `[language][string]` in the game's own encoding.
    pub raw: [[Vec<u8>; STRINGS_PER_LINE]; 5],
}

impl TextEntry {
    /// The four strings of a language, decoded to UTF-8.
    pub fn strings(&self, lang: Language) -> [String; STRINGS_PER_LINE] {
        let r = &self.raw[lang as usize];
        [decode(&r[0]), decode(&r[1]), decode(&r[2]), decode(&r[3])]
    }

    /// The non-empty strings of a language, decoded.
    pub fn lines(&self, lang: Language) -> Vec<String> {
        self.raw[lang as usize].iter().filter(|s| !s.is_empty()).map(|s| decode(s)).collect()
    }
}

/// A parsed `ILE_n.TXT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFile {
    pub entries: Vec<TextEntry>,
}

/// Extracts the four quoted strings of a language line the way the game
/// does (`0x251f9`): skip to a `"`, copy up to the next `"`, four times.
/// Anything between the strings (commas, a stray trailing comma) is ignored.
pub fn split_line(line: &[u8]) -> [Vec<u8>; STRINGS_PER_LINE] {
    let mut out: [Vec<u8>; STRINGS_PER_LINE] = Default::default();
    let mut rest = line;
    for s in out.iter_mut() {
        let Some(open) = rest.iter().position(|&b| b == b'"') else { break };
        rest = &rest[open + 1..];
        let close = rest.iter().position(|&b| b == b'"').unwrap_or(rest.len());
        *s = rest[..close].to_vec();
        rest = rest.get(close + 1..).unwrap_or(&[]);
    }
    out
}

fn trim_eol(line: &[u8]) -> &[u8] {
    let mut l = line;
    while let [head @ .., b'\r' | b'\n' | 0x1a] = l {
        l = head;
    }
    l
}

impl TextFile {
    /// Parses a whole file: repeated blocks of one id line followed by
    /// five language lines. Blank lines between blocks are skipped.
    pub fn parse(data: &[u8]) -> Result<Self, TxtError> {
        let mut lines = data.split(|&b| b == b'\n').map(trim_eol).enumerate();
        let mut entries = Vec::new();
        while let Some((n, line)) = lines.next() {
            if line.is_empty() {
                continue;
            }
            let id = std::str::from_utf8(line)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .ok_or_else(|| TxtError(format!("line {}: expected a numeric id", n + 1)))?;
            let mut raw: [[Vec<u8>; STRINGS_PER_LINE]; 5] = Default::default();
            let mut last: &[u8] = b"";
            let mut have = 0;
            for r in raw.iter_mut() {
                // The file may end inside the last entry (ILE_1.TXT id 18 has
                // no Polish line). The game's fgets then fails and leaves the
                // previous line in its buffer, which gets parsed again.
                match lines.next() {
                    Some((_, l)) if l.is_empty() && lines.clone().all(|(_, l)| l.is_empty()) => {}
                    Some((m, l)) => {
                        if !l.starts_with(b"\"") {
                            return Err(TxtError(format!("line {}: expected a quoted language line", m + 1)));
                        }
                        last = l;
                        have += 1;
                    }
                    None => {}
                }
                *r = split_line(last);
            }
            if have == 0 {
                return Err(TxtError(format!("id {id}: no language lines")));
            }
            entries.push(TextEntry { id, id_line: line.to_vec(), raw });
        }
        Ok(Self { entries })
    }

    /// Finds an entry like the game: the first line of the file that
    /// *starts with* the decimal id wins (`strstr(line, id) == line`), so
    /// id 1 would also match a line `10` if that came first. In the shipped
    /// files ids ascend, which makes this an exact match. (For an id that
    /// is absent the original never returns: it keeps calling `fgets` at
    /// end of file.)
    pub fn lookup(&self, id: u32) -> Option<&TextEntry> {
        let key = id.to_string();
        self.entries.iter().find(|e| e.id_line.starts_with(key.as_bytes()))
    }
}

/// Polish block of the game font, bytes `0xB0..=0xF1`: the Polish alphabet
/// in order (with `x`, `y` appended), lower case then upper case.
const POLISH_LOWER: [char; 33] = [
    'a', 'ą', 'b', 'c', 'ć', 'd', 'e', 'ę', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'ł', 'm', 'n', 'ń', 'o', 'p',
    'r', 's', 'ś', 't', 'u', 'ó', 'w', 'z', 'ż', 'ź', 'x', 'y',
];
const POLISH_UPPER: [char; 33] = [
    'A', 'Ą', 'B', 'C', 'Ć', 'D', 'E', 'Ę', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'Ł', 'M', 'N', 'Ń', 'O', 'P',
    'R', 'S', 'Ś', 'T', 'U', 'Ó', 'W', 'Z', 'Ż', 'Ź', 'X', 'Y',
];

/// Code page 437, `0x80..=0xAF`, which the font follows for these bytes.
const CP437_80: [char; 48] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ', 'ô', 'ö',
    'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐',
    '¬', '½', '¼', '¡', '«', '»',
];

/// Decodes one byte of game text.
///
/// * `0x20..=0x7E`: ASCII.
/// * `0x80..=0xAF`: code page 437 (all four Western languages, and `ó` in
///   Polish).
/// * `0xB0..=0xF1`: the font's own Polish block, used only by the Polish
///   lines. It matches no standard DOS or ISO code page.
/// * Everything else has no glyph and maps to U+FFFD.
pub fn decode_byte(b: u8) -> char {
    match b {
        0x20..=0x7e => b as char,
        0x80..=0xaf => CP437_80[(b - 0x80) as usize],
        0xb0..=0xd0 => POLISH_LOWER[(b - 0xb0) as usize],
        0xd1..=0xf1 => POLISH_UPPER[(b - 0xd1) as usize],
        _ => char::REPLACEMENT_CHARACTER,
    }
}

/// Decodes game text to UTF-8.
pub fn decode(text: &[u8]) -> String {
    text.iter().map(|&b| decode_byte(b)).collect()
}

/// Encodes a character back to the game's byte, preferring the lowest
/// byte value (so plain letters stay ASCII). `None` if the font lacks it.
pub fn encode_char(c: char) -> Option<u8> {
    (0x20..=0xf1u8).find(|&b| b != 0x7f && decode_byte(b) == c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_the_game() {
        let s = split_line(b"\"Et hop\",\"\",\"\",\"\",");
        assert_eq!(s, [b"Et hop".to_vec(), vec![], vec![], vec![]]);
        let s = split_line(b"\"a, b\",\"c\",\"d\",\"e\",\"ignored\"");
        assert_eq!(s, [b"a, b".to_vec(), b"c".to_vec(), b"d".to_vec(), b"e".to_vec()]);
        let s = split_line(b"\"only\"");
        assert_eq!(s[0], b"only");
        assert!(s[1].is_empty());
    }

    #[test]
    fn decodes_charset() {
        assert_eq!(decode(b"tr\x8as utile"), "très utile");
        assert_eq!(decode(b"\xadHop! \xa3til"), "¡Hop! útil");
        assert_eq!(decode(b"sch\x94nen H\x84nde, n\x81tzlich"), "schönen Hände, nützlich");
        assert_eq!(decode(b"wezm\xb7 u\xcdyteczny \xc7liczne r\xb1czki pude\xbfko"), "wezmę użyteczny śliczne rączki pudełko");
        assert_eq!(decode(b"znale\xce\xb4 potwor\xa2w \xefle \xe8wietnie ko\xc2ca"), "znaleźć potworów Źle Świetnie końca");
        assert_eq!(decode_byte(0xf2), char::REPLACEMENT_CHARACTER);
        for b in 0x20..=0xf1u8 {
            let c = decode_byte(b);
            if c != char::REPLACEMENT_CHARACTER {
                assert_eq!(decode_byte(encode_char(c).unwrap()), c);
            }
        }
        assert_eq!(encode_char('a'), Some(b'a'));
        assert_eq!(encode_char('ę'), Some(0xb7));
        assert_eq!(encode_char('ó'), Some(0xa2));
    }

    #[test]
    fn parses_entries() {
        let src = b"0\r\n\"a\",\"\",\"\",\"\"\r\n\"b\",\"\",\"\",\"\"\r\n\"c\",\"\",\"\",\"\"\r\n\"d\",\"\",\"\",\"\"\r\n\"e\",\"f\",\"\",\"\"\r\n\r\n";
        let f = TextFile::parse(src).unwrap();
        assert_eq!(f.entries.len(), 1);
        assert_eq!(f.entries[0].lines(Language::Polish), ["e", "f"]);
        assert_eq!(f.lookup(0).unwrap().strings(Language::English)[0], "b");
        assert!(f.lookup(1).is_none());
        // A file that ends early repeats the last line it has.
        let f = TextFile::parse(b"7\r\n\"a\"\r\n\"b\"\r\n").unwrap();
        assert_eq!(f.entries[0].lines(Language::French), ["a"]);
        assert_eq!(f.entries[0].lines(Language::Polish), ["b"]);
        assert!(TextFile::parse(b"0\r\n").is_err());
        assert!(TextFile::parse(b"0\r\n\"a\"\r\n1\r\n").is_err());
        assert!(TextFile::parse(b"x\r\n").is_err());
    }

    #[test]
    fn disc_texts() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        let counts = [19, 37, 42, 40, 22];
        for (n, count) in (1..=5).zip(counts) {
            let Ok(data) = std::fs::read(format!("{dir}ILE_{n}.TXT")) else {
                eprintln!("skipping: disc files not present");
                return;
            };
            let f = TextFile::parse(&data).unwrap_or_else(|e| panic!("ILE_{n}.TXT: {e}"));
            assert_eq!(f.entries.len(), count, "ILE_{n}.TXT");
            // Ids ascend from 0, so the game's prefix match is exact.
            // Only ILE_5.TXT has a gap: it lacks 10 and 11.
            let ids: Vec<u32> = f.entries.iter().map(|e| e.id).collect();
            let expect: Vec<u32> = match n {
                5 => (0..10).chain(12..24).collect(),
                _ => (0..count as u32).collect(),
            };
            assert_eq!(ids, expect, "ILE_{n}.TXT");
            for e in &f.entries {
                assert_eq!(f.lookup(e.id).unwrap().id, e.id);
                for lang in Language::ALL {
                    for s in e.strings(lang) {
                        assert!(!s.contains(char::REPLACEMENT_CHARACTER), "ILE_{n}.TXT id {}: {s}", e.id);
                    }
                    // Only the Polish lines use the font's Polish block.
                    let high = e.raw[lang as usize].iter().flatten().any(|&b| b >= 0xb0);
                    assert!(!high || lang == Language::Polish, "ILE_{n}.TXT id {}", e.id);
                }
            }
        }
        let data = std::fs::read(format!("{dir}ILE_2.TXT")).unwrap();
        let f = TextFile::parse(&data).unwrap();
        assert_eq!(f.entries[0].lines(Language::French), ["Cette machette me semble très utile", "et hop je la prends!!!"]);
        assert_eq!(f.entries[0].lines(Language::Polish)[0], "Ten toporek na bardzo użyteczny");
    }
}
