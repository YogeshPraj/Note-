//! File encoding and line-ending detection. Portable, no Scintilla involved.

/// A legacy single- or double-byte character set, from Notepad++'s "Character sets" menu.
pub struct Charset {
    pub group: &'static str,
    pub label: &'static str,
    /// Stable name stored in sessions.
    pub key: &'static str,
    pub enc: &'static encoding_rs::Encoding,
}

macro_rules! cs {
    ($g:literal, $l:literal, $k:literal, $e:ident) => {
        Charset { group: $g, label: $l, key: $k, enc: encoding_rs::$e }
    };
}

/// In menu order, grouped by region. (Windows-1252 is `Enc::Ansi`.)
pub const CHARSETS: &[Charset] = &[
    cs!("Arabic", "ISO 8859-6", "cs:iso-8859-6", ISO_8859_6),
    cs!("Arabic", "Windows-1256", "cs:windows-1256", WINDOWS_1256),
    cs!("Baltic", "ISO 8859-4", "cs:iso-8859-4", ISO_8859_4),
    cs!("Baltic", "ISO 8859-13", "cs:iso-8859-13", ISO_8859_13),
    cs!("Baltic", "Windows-1257", "cs:windows-1257", WINDOWS_1257),
    cs!("Celtic", "ISO 8859-14", "cs:iso-8859-14", ISO_8859_14),
    cs!("Central European", "ISO 8859-2", "cs:iso-8859-2", ISO_8859_2),
    cs!("Central European", "Windows-1250", "cs:windows-1250", WINDOWS_1250),
    cs!("Chinese", "Big5", "cs:big5", BIG5),
    cs!("Chinese", "GBK", "cs:gbk", GBK),
    cs!("Chinese", "GB18030", "cs:gb18030", GB18030),
    cs!("Cyrillic", "ISO 8859-5", "cs:iso-8859-5", ISO_8859_5),
    cs!("Cyrillic", "KOI8-R", "cs:koi8-r", KOI8_R),
    cs!("Cyrillic", "KOI8-U", "cs:koi8-u", KOI8_U),
    cs!("Cyrillic", "Windows-1251", "cs:windows-1251", WINDOWS_1251),
    cs!("Cyrillic", "OEM 866", "cs:ibm866", IBM866),
    cs!("Cyrillic", "Mac Cyrillic", "cs:x-mac-cyrillic", X_MAC_CYRILLIC),
    cs!("Eastern European", "ISO 8859-16", "cs:iso-8859-16", ISO_8859_16),
    cs!("Greek", "ISO 8859-7", "cs:iso-8859-7", ISO_8859_7),
    cs!("Greek", "Windows-1253", "cs:windows-1253", WINDOWS_1253),
    cs!("Hebrew", "ISO 8859-8", "cs:iso-8859-8", ISO_8859_8),
    cs!("Hebrew", "Windows-1255", "cs:windows-1255", WINDOWS_1255),
    cs!("Japanese", "Shift-JIS", "cs:shift_jis", SHIFT_JIS),
    cs!("Japanese", "EUC-JP", "cs:euc-jp", EUC_JP),
    cs!("Korean", "EUC-KR", "cs:euc-kr", EUC_KR),
    cs!("North European", "ISO 8859-10", "cs:iso-8859-10", ISO_8859_10),
    cs!("Thai", "Windows-874", "cs:windows-874", WINDOWS_874),
    cs!("Turkish", "Windows-1254", "cs:windows-1254", WINDOWS_1254),
    cs!("Vietnamese", "Windows-1258", "cs:windows-1258", WINDOWS_1258),
    cs!("Western European", "ISO 8859-3", "cs:iso-8859-3", ISO_8859_3),
    cs!("Western European", "ISO 8859-15", "cs:iso-8859-15", ISO_8859_15),
    cs!("Western European", "Macintosh", "cs:macintosh", MACINTOSH),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Ansi,
    /// Index into `CHARSETS`.
    Cs(u8),
}

impl Enc {
    pub fn label(self) -> &'static str {
        match self {
            Enc::Utf8 => "UTF-8",
            Enc::Utf8Bom => "UTF-8-BOM",
            Enc::Utf16Le => "UTF-16 LE BOM",
            Enc::Utf16Be => "UTF-16 BE BOM",
            Enc::Ansi => "ANSI",
            Enc::Cs(i) => CHARSETS[i as usize].label,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf8",
            Enc::Utf8Bom => "utf8bom",
            Enc::Utf16Le => "utf16le",
            Enc::Utf16Be => "utf16be",
            Enc::Ansi => "ansi",
            Enc::Cs(i) => CHARSETS[i as usize].key,
        }
    }
    pub fn from_key(k: &str) -> Enc {
        match k {
            "utf8bom" => Enc::Utf8Bom,
            "utf16le" => Enc::Utf16Le,
            "utf16be" => Enc::Utf16Be,
            "ansi" => Enc::Ansi,
            _ => CHARSETS.iter().position(|c| c.key == k).map(|i| Enc::Cs(i as u8)).unwrap_or(Enc::Utf8),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eol {
    Crlf,
    Lf,
    Cr,
}

impl Eol {
    pub fn label(self) -> &'static str {
        match self {
            Eol::Crlf => "Windows (CR LF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Eol::Crlf => "crlf",
            Eol::Lf => "lf",
            Eol::Cr => "cr",
        }
    }
    pub fn from_key(k: &str) -> Eol {
        match k {
            "lf" => Eol::Lf,
            "cr" => Eol::Cr,
            _ => Eol::Crlf,
        }
    }
    /// Value for SCI_SETEOLMODE / SCI_CONVERTEOLS.
    pub fn sci_mode(self) -> usize {
        match self {
            Eol::Crlf => 0,
            Eol::Cr => 1,
            Eol::Lf => 2,
        }
    }
    pub fn platform_default() -> Eol {
        if cfg!(windows) { Eol::Crlf } else { Eol::Lf }
    }
}

pub fn detect_eol(text: &str) -> Eol {
    let b = text.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                crlf += 1;
                i += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if crlf + lf + cr == 0 {
        Eol::platform_default()
    } else if crlf >= lf && crlf >= cr {
        Eol::Crlf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    }
}

/// UTF-16 text without a byte-order mark: mostly Latin text has a zero byte next to every
/// letter. `Some(true)` for little-endian, `Some(false)` for big-endian.
fn utf16_without_bom(bytes: &[u8]) -> Option<bool> {
    let sample = &bytes[..bytes.len().min(4096) & !1];
    if sample.len() < 4 {
        return None;
    }
    let pairs = sample.len() / 2;
    let (mut little, mut big) = (0usize, 0usize);
    for p in sample.chunks_exact(2) {
        match (p[0], p[1]) {
            (a, 0) if a != 0 => little += 1,
            (0, b) if b != 0 => big += 1,
            _ => {}
        }
    }
    // Most pairs must be "letter + zero", the other way round hardly ever.
    if little * 10 >= pairs * 7 && big * 20 <= pairs {
        Some(true)
    } else if big * 10 >= pairs * 7 && little * 20 <= pairs {
        Some(false)
    } else {
        None
    }
}

pub fn decode(bytes: &[u8]) -> (String, Enc, Eol) {
    let (text, enc) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(rest).into_owned(), Enc::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (encoding_rs::UTF_16LE.decode_without_bom_handling(rest).0.into_owned(), Enc::Utf16Le)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (encoding_rs::UTF_16BE.decode_without_bom_handling(rest).0.into_owned(), Enc::Utf16Be)
    } else if let Some(little) = utf16_without_bom(bytes) {
        if little {
            (encoding_rs::UTF_16LE.decode_without_bom_handling(bytes).0.into_owned(), Enc::Utf16Le)
        } else {
            (encoding_rs::UTF_16BE.decode_without_bom_handling(bytes).0.into_owned(), Enc::Utf16Be)
        }
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        (s.to_owned(), Enc::Utf8)
    } else {
        (encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned(), Enc::Ansi)
    };
    let eol = detect_eol(&text);
    (text, enc, eol)
}

pub fn encode(text: &str, enc: Enc) -> Vec<u8> {
    match enc {
        Enc::Utf8 => text.as_bytes().to_vec(),
        Enc::Utf8Bom => [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat(),
        Enc::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            out.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
            out
        }
        Enc::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            out.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
            out
        }
        Enc::Ansi => encode_legacy(encoding_rs::WINDOWS_1252, text),
        Enc::Cs(i) => encode_legacy(CHARSETS[i as usize].enc, text),
    }
}

/// Encode for a legacy character set. A character the set cannot hold becomes ? (what
/// Notepad++ writes), never an HTML numeric reference, which would corrupt the file.
fn encode_legacy(e: &'static encoding_rs::Encoding, text: &str) -> Vec<u8> {
    let (out, _, unmappable) = e.encode(text);
    if !unmappable {
        return out.into_owned();
    }
    let mut bytes = Vec::with_capacity(text.len());
    let mut buf = [0u8; 4];
    for c in text.chars() {
        let (o, _, bad) = e.encode(c.encode_utf8(&mut buf));
        if bad {
            bytes.push(b'?');
        } else {
            bytes.extend_from_slice(&o);
        }
    }
    bytes
}

/// Interpret `bytes` as `enc` (the user's choice, not a guess). A BOM that
/// matches the encoding is dropped.
pub fn decode_with(bytes: &[u8], enc: Enc) -> (String, Eol) {
    let text = match enc {
        Enc::Utf8 | Enc::Utf8Bom => String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned(),
        Enc::Utf16Le => encoding_rs::UTF_16LE.decode_without_bom_handling(bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes)).0.into_owned(),
        Enc::Utf16Be => encoding_rs::UTF_16BE.decode_without_bom_handling(bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes)).0.into_owned(),
        Enc::Ansi => encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned(),
        Enc::Cs(i) => CHARSETS[i as usize].enc.decode_without_bom_handling(bytes).0.into_owned(),
    };
    let eol = detect_eol(&text);
    (text, eol)
}

/// Can every character be written in `enc`? (Unicode encodings always can; the
/// legacy ones would otherwise replace the characters with `?`.)
pub fn can_encode(text: &str, enc: Enc) -> bool {
    match enc {
        Enc::Ansi => !encoding_rs::WINDOWS_1252.encode(text).2,
        Enc::Cs(i) => !CHARSETS[i as usize].enc.encode(text).2,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmappable_characters_become_question_marks() {
        assert_eq!(encode("a日b", Enc::Ansi), b"a?b");
        assert_eq!(encode("é日", Enc::Ansi), b"\xE9?");
    }

    #[test]
    fn utf16_without_a_bom_is_recognised() {
        let le: Vec<u8> = "héllo wörld, plain text".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let be: Vec<u8> = "hello world, plain text".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        assert_eq!(decode(&le).1, Enc::Utf16Le);
        assert_eq!(decode(&le).0, "héllo wörld, plain text");
        assert_eq!(decode(&be).1, Enc::Utf16Be);
        // Ordinary text, even with a stray NUL, stays UTF-8.
        assert_eq!(decode(b"plain text with a \0 NUL").1, Enc::Utf8);
        assert_eq!(decode("日本語のテキスト".as_bytes()).1, Enc::Utf8);
    }

    #[test]
    fn detects_encodings() {
        assert_eq!(decode(b"hello").1, Enc::Utf8);
        assert_eq!(decode(b"\xEF\xBB\xBFhi").1, Enc::Utf8Bom);
        assert_eq!(decode(b"\xEF\xBB\xBFhi").0, "hi");
        assert_eq!(decode(b"caf\xE9").1, Enc::Ansi);
        assert_eq!(decode(b"caf\xE9").0, "café");
    }

    #[test]
    fn round_trips_every_encoding() {
        let text = "héllo\r\nwörld € ✓";
        for enc in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be] {
            let (back, e, _) = decode(&encode(text, enc));
            assert_eq!(back, text, "{enc:?}");
            assert_eq!(e, enc);
        }
        let (back, e, _) = decode(&encode("café", Enc::Ansi));
        assert_eq!((back.as_str(), e), ("café", Enc::Ansi));
    }

    #[test]
    fn detects_eol_majority() {
        assert_eq!(detect_eol("a\r\nb\r\nc\n"), Eol::Crlf);
        assert_eq!(detect_eol("a\nb\nc\r\n"), Eol::Lf);
        assert_eq!(detect_eol("a\rb\rc"), Eol::Cr);
        assert_eq!(detect_eol("none"), Eol::platform_default());
    }

    #[test]
    fn keys_round_trip() {
        let mut all = vec![Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi];
        all.extend((0..CHARSETS.len()).map(|i| Enc::Cs(i as u8)));
        for e in all {
            assert_eq!(Enc::from_key(e.key()), e);
        }
        for e in [Eol::Crlf, Eol::Lf, Eol::Cr] {
            assert_eq!(Eol::from_key(e.key()), e);
        }
    }

    #[test]
    fn charsets_decode_and_encode() {
        let win1251 = Enc::from_key("cs:windows-1251");
        let bytes = encode("Привет", win1251);
        assert_eq!(bytes, [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        assert_eq!(decode_with(&bytes, win1251).0, "Привет");
        let sjis = Enc::from_key("cs:shift_jis");
        assert_eq!(decode_with(&encode("日本語", sjis), sjis).0, "日本語");
        // The same bytes read as another character set give different text.
        assert_ne!(decode_with(&bytes, Enc::Ansi).0, "Привет");
    }

    #[test]
    fn reinterpreting_drops_a_matching_bom() {
        assert_eq!(decode_with(b"\xEF\xBB\xBFhi", Enc::Utf8).0, "hi");
        assert_eq!(decode_with(b"\xFF\xFEh\0i\0", Enc::Utf16Le).0, "hi");
        assert_eq!(decode_with(b"\xFE\xFF\0h\0i", Enc::Utf16Be).0, "hi");
        assert_eq!(decode_with(b"a\r\nb", Enc::Utf8).1, Eol::Crlf);
    }

    #[test]
    fn unrepresentable_text_is_detected() {
        assert!(can_encode("café", Enc::Ansi));
        assert!(!can_encode("日本", Enc::Ansi));
        assert!(!can_encode("日本", Enc::from_key("cs:windows-1251")));
        assert!(can_encode("日本", Enc::from_key("cs:shift_jis")));
        assert!(can_encode("日本", Enc::Utf8));
    }
}
