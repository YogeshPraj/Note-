use super::*;

#[test]
fn palettes_stay_inside_lexer_style_range() {
    for (_, l) in LANGUAGES {
        let limit = if *l == "phpscript" { 128 } else { 32 };
        assert!(palette(l, &Theme::light()).iter().all(|(s, _)| *s < limit), "{l}");
    }
}

#[test]
fn labels_are_unique_and_resolve() {
    let mut seen = std::collections::HashSet::new();
    for (label, lexer) in LANGUAGES {
        assert!(seen.insert(*label), "duplicate {label}");
        assert_eq!(lexer_of_label(label), *lexer);
    }
    use std::path::Path;
    assert_eq!(label_for("cpp", Some(Path::new("a.js"))), "JavaScript");
    assert_eq!(label_for("cpp", Some(Path::new("a.cpp"))), "C++");
    assert_eq!(label_for("cpp", Some(Path::new("a.c"))), "C");
    assert_eq!(label_for("rust", Some(Path::new("a.rs"))), "Rust");
    assert_eq!(label_for("null", None), "Normal text");
    assert_eq!(label_for("hypertext", None), "HTML");
}

#[test]
fn display_names() {
    assert_eq!(display_name("null"), "Normal text");
    assert_eq!(display_name("rust"), "Rust");
    assert_eq!(display_name("json"), "JSON");
}
