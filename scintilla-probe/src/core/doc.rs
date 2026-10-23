//! File encoding and line-ending detection. Portable, no Scintilla involved.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Ansi,
}

impl Enc {
    pub fn label(self) -> &'static str {
        match self {
            Enc::Utf8 => "UTF-8",
            Enc::Utf8Bom => "UTF-8-BOM",
            Enc::Utf16Le => "UTF-16 LE BOM",
            Enc::Utf16Be => "UTF-16 BE BOM",
            Enc::Ansi => "ANSI",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf8",
            Enc::Utf8Bom => "utf8bom",
            Enc::Utf16Le => "utf16le",
            Enc::Utf16Be => "utf16be",
            Enc::Ansi => "ansi",
        }
    }
    pub fn from_key(k: &str) -> Enc {
        match k {
            "utf8bom" => Enc::Utf8Bom,
            "utf16le" => Enc::Utf16Le,
            "utf16be" => Enc::Utf16Be,
            "ansi" => Enc::Ansi,
            _ => Enc::Utf8,
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

pub fn decode(bytes: &[u8]) -> (String, Enc, Eol) {
    let (text, enc) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(rest).into_owned(), Enc::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (encoding_rs::UTF_16LE.decode_without_bom_handling(rest).0.into_owned(), Enc::Utf16Le)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (encoding_rs::UTF_16BE.decode_without_bom_handling(rest).0.into_owned(), Enc::Utf16Be)
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
        Enc::Ansi => encoding_rs::WINDOWS_1252.encode(text).0.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for e in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi] {
            assert_eq!(Enc::from_key(e.key()), e);
        }
        for e in [Eol::Crlf, Eol::Lf, Eol::Cr] {
            assert_eq!(Eol::from_key(e.key()), e);
        }
    }
}
