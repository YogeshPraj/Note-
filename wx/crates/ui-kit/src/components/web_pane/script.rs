//! Building JavaScript to send to a page: quoting, calls and chunking. No toolkit in it, so it is unit tested.

/// `s` as a JavaScript string literal.
pub fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// `path(arg1,arg2,...)` with every argument quoted as a string.
pub fn call(path: &str, args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|a| js_string(a)).collect();
    format!("{path}({})", args.join(","))
}

/// Split `js` into pieces of about `size` bytes without cutting a UTF-8 character.
pub fn chunks(js: &str, size: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < js.len() {
        let mut end = (start + size.max(1)).min(js.len());
        while !js.is_char_boundary(end) {
            end += 1;
        }
        out.push(&js[start..end]);
        start = end;
    }
    out
}

/// The statements that hand `js` to the page in pieces and evaluate it once in the global scope. Large libraries
/// cannot go through one script call.
pub fn big_script(js: &str, size: usize) -> Vec<String> {
    let mut v = vec!["window.__big=[]".to_string()];
    v.extend(chunks(js, size).into_iter().map(|c| format!("window.__big.push({})", js_string(c))));
    v.push("(function(){var s=document.createElement('script');s.textContent=window.__big.join('');document.head.appendChild(s);window.__big=null})()".into());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_quoted_and_escaped() {
        assert_eq!(js_string("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
        assert_eq!(js_string(""), "\"\"");
    }

    #[test]
    fn a_closing_script_tag_in_content_stays_inert_in_a_string_literal() {
        // Inside a quoted JS string `</script>` is harmless to the evaluator; it must simply round-trip.
        let lit = js_string("</script><b>");
        assert!(lit.starts_with('"') && lit.ends_with('"'));
        let back: String = serde_json::from_str(&lit).unwrap();
        assert_eq!(back, "</script><b>");
    }

    #[test]
    fn calls_quote_every_argument() {
        assert_eq!(call("window.np.render", &["markdown", "# hi \"x\""]), r##"window.np.render("markdown","# hi \"x\"")"##);
        assert_eq!(call("f", &[]), "f()");
    }

    #[test]
    fn chunks_rejoin_to_the_original_and_respect_the_size() {
        let s = "abcdefghij".repeat(7);
        let c = chunks(&s, 16);
        assert_eq!(c.concat(), s);
        assert!(c.iter().all(|p| p.len() <= 16));
    }

    #[test]
    fn chunks_never_split_a_multibyte_character() {
        let s = "h\u{e9}llo \u{4e16}\u{754c} \u{1f600} done".repeat(5);
        for size in [1, 2, 3, 5, 7] {
            let c = chunks(&s, size);
            assert_eq!(c.concat(), s);
            assert!(c.iter().all(|p| std::str::from_utf8(p.as_bytes()).is_ok()));
        }
    }

    #[test]
    fn empty_input_has_no_chunks() {
        assert!(chunks("", 10).is_empty());
    }

    #[test]
    fn a_big_script_starts_clean_pushes_every_piece_and_then_runs() {
        let v = big_script(&"x".repeat(25), 10);
        assert_eq!(v.first().unwrap(), "window.__big=[]");
        assert_eq!(v.iter().filter(|s| s.starts_with("window.__big.push(")).count(), 3);
        assert!(v.last().unwrap().contains("createElement('script')"));
    }
}
