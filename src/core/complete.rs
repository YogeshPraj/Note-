//! Auto-completion sources: words and function names found in the document,
//! file-system paths, and a one-line signature for the call tip.

use std::path::{Path, MAIN_SEPARATOR};

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The identifier ending at byte offset `pos`, with the offset it starts at.
pub fn word_before(text: &str, pos: usize) -> (usize, &str) {
    let head = &text[..pos.min(text.len())];
    let start = head.char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map(|(i, _)| i).unwrap_or(head.len());
    (start, &head[start..])
}

/// Every identifier in `text` as (byte offset, word, followed by an opening bracket).
fn identifiers(text: &str) -> Vec<(usize, &str, bool)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        match (is_word(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &text[s..i], c == '('));
                start = None;
            }
            _ => {}
        }
    }
    out
}

fn collect(text: &str, prefix: &str, typing_at: usize, only_calls: bool) -> Vec<String> {
    let lower = prefix.to_lowercase();
    let mut words: Vec<String> = identifiers(text)
        .into_iter()
        .filter(|(at, w, call)| {
            *at != typing_at && w.chars().count() > prefix.chars().count() && (!only_calls || *call) && w.to_lowercase().starts_with(&lower)
        })
        .map(|(_, w, _)| w.to_string())
        .collect();
    words.sort_by_key(|w| w.to_lowercase());
    words.dedup();
    words
}

/// Words in the document that continue `prefix` (the word being typed is skipped).
pub fn words_with_prefix(text: &str, prefix: &str, typing_at: usize) -> Vec<String> {
    collect(text, prefix, typing_at, false)
}

/// Names that are called somewhere in the document, i.e. followed by `(`.
pub fn function_names(text: &str, prefix: &str, typing_at: usize) -> Vec<String> {
    collect(text, prefix, typing_at, true)
}

/// The path-like text just before the caret on its line.
pub fn path_fragment(line_before_caret: &str) -> &str {
    let start = line_before_caret.rfind(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '|' | '*' | '?' | '(' | ')' | '=')).map(|i| i + line_before_caret[i..].chars().next().map_or(1, char::len_utf8)).unwrap_or(0);
    &line_before_caret[start..]
}

/// Entries of the directory named by `fragment` that continue its last component.
/// Returns the length (in bytes) of the typed component and the candidates.
pub fn path_candidates(fragment: &str, base: Option<&Path>) -> (usize, Vec<String>) {
    let cut = fragment.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let (dir_part, name) = fragment.split_at(cut);
    let dir = if dir_part.is_empty() {
        match base {
            Some(b) => b.to_path_buf(),
            None => return (name.len(), Vec::new()),
        }
    } else if Path::new(dir_part).is_absolute() || base.is_none() {
        Path::new(dir_part).to_path_buf()
    } else {
        base.unwrap().join(dir_part)
    };
    let lower = name.to_lowercase();
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            if !n.to_lowercase().starts_with(&lower) {
                return None;
            }
            let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(if dir { format!("{n}{MAIN_SEPARATOR}") } else { n })
        })
        .collect();
    found.sort_by_key(|n| n.to_lowercase());
    (name.len(), found)
}

/// The definition line of `name(...)`, shown as a call tip.
pub fn signature_for(text: &str, name: &str) -> Option<String> {
    let call = format!("{name}(");
    const KEYWORDS: [&str; 7] = ["fn", "def", "function", "func", "sub", "proc", "fun"];
    text.lines().map(str::trim).find_map(|line| {
        let at = line.find(&call)?;
        if at > 0 && is_word(line[..at].chars().next_back()?) {
            return None;
        }
        let before: Vec<&str> = line[..at].split_whitespace().collect();
        let keyword = before.last().is_some_and(|w| KEYWORDS.contains(w));
        // C-like: "int add(int a, int b) {" — a type before the name, no assignment, not a plain call.
        let c_like = !before.is_empty() && !line[..at].contains(['=', '.', '(', ',']) && !line.ends_with(';');
        (keyword || c_like).then(|| line.trim_end_matches('{').trim_end().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_word_being_typed() {
        assert_eq!(word_before("let foo_ba", 10), (4, "foo_ba"));
        assert_eq!(word_before("a.b", 2), (2, ""));
        assert_eq!(word_before("", 0), (0, ""));
    }

    #[test]
    fn completes_from_document_words() {
        let t = "alpha alphabet Alpine al beta";
        let typing = t.find("al beta").unwrap();
        assert_eq!(words_with_prefix(t, "al", typing), ["alpha", "alphabet", "Alpine"]);
        assert!(words_with_prefix(t, "zz", typing).is_empty());
    }

    #[test]
    fn function_completion_uses_called_names() {
        let t = "render(x); renderAll(y); renderer = 1;";
        assert_eq!(function_names(t, "ren", usize::MAX), ["render", "renderAll"]);
    }

    #[test]
    fn path_fragment_is_the_last_path_like_run() {
        assert_eq!(path_fragment("open C:\\Users\\yo"), "C:\\Users\\yo");
        assert_eq!(path_fragment("src=\"./sty"), "./sty");
        assert_eq!(path_fragment("plain"), "plain");
    }

    #[test]
    fn path_candidates_lists_matching_entries() {
        let d = std::env::temp_dir().join(format!("npp_complete_{}", std::process::id()));
        let _ = std::fs::create_dir_all(d.join("alpha_dir"));
        std::fs::write(d.join("alpine.txt"), "x").unwrap();
        std::fs::write(d.join("beta.txt"), "x").unwrap();
        let frag = format!("{}{}al", d.display(), MAIN_SEPARATOR);
        let (typed, found) = path_candidates(&frag, None);
        assert_eq!(typed, 2);
        assert_eq!(found, [format!("alpha_dir{MAIN_SEPARATOR}"), "alpine.txt".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn signatures() {
        let t = "fn add(a: i32, b: i32) -> i32 {\n    add(1, 2)\n}\nint sub(int a) {\n  return sub(1);\n}";
        assert_eq!(signature_for(t, "add").unwrap(), "fn add(a: i32, b: i32) -> i32");
        assert_eq!(signature_for(t, "sub").unwrap(), "int sub(int a) {".trim_end_matches('{').trim_end());
        assert_eq!(signature_for(t, "nope"), None);
    }
}
