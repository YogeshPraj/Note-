//! Guess the language of pasted text, for a tab that has no file name to tell it by.
//!
//! The answer is a Language-menu label (see `lang::LANGUAGES`), or `None` when nothing stands out: a wrong guess is
//! worse than plain text. Structured formats (JSON, XML, HTML, diffs, shebangs) are recognised first; the rest are
//! scored by the tokens each language is known for.

/// How much of the text is looked at.
const SAMPLE: usize = 20_000;
/// A language wins with at least this score, and at least `LEAD` points more than the runner-up.
const MIN_SCORE: u32 = 4;
const LEAD: u32 = 2;

mod rules;
use rules::RULES;
use rules::Token::{self, Ends, Has, Starts};

/// The Language-menu label for `text`, if it can be told.
pub fn detect_language(text: &str) -> Option<&'static str> {
    let sample: String = text.chars().take(SAMPLE).collect();
    let body = sample.trim();
    if body.chars().count() < 8 {
        return None;
    }
    structured(body, text).or_else(|| scored(body))
}

fn structured(body: &str, whole: &str) -> Option<&'static str> {
    shebang(body)
        .or_else(|| json(body, whole))
        .or_else(|| markup(body))
        .or_else(|| diff(body))
        .or_else(|| sql(body))
        .or_else(|| markdown(body))
        .or_else(|| css(body))
        .or_else(|| yaml(body))
}

fn shebang(body: &str) -> Option<&'static str> {
    let first = body.lines().next()?;
    if !first.starts_with("#!") {
        return None;
    }
    let f = first.to_ascii_lowercase();
    [
        ("python", "Python"),
        ("pwsh", "PowerShell"),
        ("powershell", "PowerShell"),
        ("node", "JavaScript"),
        ("ruby", "Ruby"),
        ("perl", "Perl"),
        ("lua", "Lua"),
        ("bash", "Shell"),
        ("/sh", "Shell"),
        ("zsh", "Shell"),
        ("env sh", "Shell"),
    ]
    .into_iter()
    .find(|(k, _)| f.contains(k))
    .map(|(_, l)| l)
}

fn json(body: &str, whole: &str) -> Option<&'static str> {
    let opens = body.starts_with('{') || body.starts_with('[');
    (opens && serde_json::from_str::<serde_json::Value>(whole.trim()).is_ok()).then_some("JSON")
}

fn markup(body: &str) -> Option<&'static str> {
    if !body.starts_with('<') {
        return None;
    }
    let head: String = body.chars().take(2_000).collect::<String>().to_ascii_lowercase();
    if head.starts_with("<?php") {
        return Some("PHP");
    }
    const HTML: [&str; 12] = ["<!doctype html", "<html", "<body", "<div", "<script", "<head", "<p>", "<span", "<a href", "<table", "<ul", "<li"];
    if HTML.iter().any(|t| head.contains(t)) {
        return Some("HTML");
    }
    (head.starts_with("<?xml") || (body.ends_with('>') && (head.contains("</") || head.contains("/>")))).then_some("XML")
}

fn diff(body: &str) -> Option<&'static str> {
    let has = |p: &str| body.lines().any(|l| l.starts_with(p));
    (has("diff --git ") || (has("--- ") && has("+++ ") && has("@@ "))).then_some("Diff")
}

fn sql(body: &str) -> Option<&'static str> {
    let first = body.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("--"))?.to_ascii_uppercase();
    let lower = body.to_ascii_lowercase();
    let starts = ["SELECT ", "INSERT INTO ", "UPDATE ", "DELETE FROM ", "CREATE TABLE ", "CREATE INDEX ", "ALTER TABLE ", "DROP TABLE ", "WITH "];
    let backed = [" from ", " where ", " values", " set ", " table ", " as (", ";"];
    let backing = backed.iter().filter(|b| lower.contains(*b)).count();
    (starts.iter().any(|s| first.starts_with(s)) && (backing >= 2 || body.trim_end().ends_with(';'))).then_some("SQL")
}

fn markdown(body: &str) -> Option<&'static str> {
    let lines: Vec<&str> = body.lines().map(str::trim_start).collect();
    let heading = lines.iter().any(|l| l.starts_with('#') && l.trim_start_matches('#').starts_with(' ') && l.len() - l.trim_start_matches('#').len() <= 6);
    if !heading {
        return None;
    }
    let kinds = [
        lines.iter().any(|l| l.starts_with("```")),
        lines.iter().any(|l| l.starts_with("- ") || l.starts_with("* ") || l.starts_with("1. ")),
        body.contains("]("),
        body.contains("**"),
        lines.iter().any(|l| l.starts_with("> ")),
        lines.iter().filter(|l| l.starts_with("# ")).count() >= 2,
    ];
    (kinds.iter().filter(|k| **k).count() >= 1 && !body.contains(';')).then_some("Markdown")
}

fn css(body: &str) -> Option<&'static str> {
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    let selectors = lines.iter().filter(|l| l.ends_with('{') && !l.contains(['=', '(', '"']) && !l.starts_with("function") && !l.starts_with("if")).count();
    let decls = lines
        .iter()
        .filter(|l| l.ends_with(';') && l.split_once(':').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '-')))
        .count();
    (selectors >= 1 && decls >= 2 && !body.contains("function") && !body.contains("=>")).then_some("CSS")
}

fn yaml(body: &str) -> Option<&'static str> {
    const NOT_KEYS: [&str; 5] = ["else", "try", "finally", "default", "case"];
    let lines: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if lines.len() < 3 || body.contains(';') || lines.iter().any(|l| l.ends_with('{') || l.ends_with('}')) {
        return None;
    }
    let keyed = |l: &str| {
        let l = l.strip_prefix("- ").unwrap_or(l);
        l.split_once(':').is_some_and(|(k, v)| {
            let k = k.trim_matches(['"', '\'']);
            !k.is_empty()
                && !NOT_KEYS.contains(&k)
                && k.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                && (v.is_empty() || v.starts_with(' '))
        })
    };
    let matching = lines.iter().filter(|l| keyed(l) || l.starts_with("- ") || **l == "---").count();
    (matching * 4 >= lines.len() * 3 && lines.iter().filter(|l| keyed(l)).count() >= 2).then_some("YAML")
}

fn present(token: Token, lines: &[&str], body: &str) -> bool {
    match token {
        Has(s) => body.contains(s),
        Starts(s) => lines.iter().any(|l| l.starts_with(s)),
        Ends(s) => lines.iter().any(|l| l.ends_with(s)),
    }
}

fn scored(body: &str) -> Option<&'static str> {
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    let mut scores: Vec<(u32, &'static str)> =
        RULES.iter().map(|r| (r.tokens.iter().filter(|(t, _)| present(*t, &lines, body)).map(|(_, w)| w).sum(), r.label)).collect();
    scores.sort_by_key(|s| std::cmp::Reverse(s.0));
    let (best, runner_up) = (scores.first()?, scores.get(1).map_or(0, |s| s.0));
    (best.0 >= MIN_SCORE && best.0 >= runner_up + LEAD).then_some(best.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is(text: &str, label: &str) {
        assert_eq!(detect_language(text), Some(label), "{text}");
    }

    #[test]
    fn structured_formats_are_recognised() {
        is("{\"name\": \"x\", \"items\": [1, 2, 3]}", "JSON");
        is("[1, 2, 3, 4]", "JSON");
        is("<?xml version=\"1.0\"?>\n<root><a>1</a></root>", "XML");
        is("<!DOCTYPE html>\n<html><body><div>hi</div></body></html>", "HTML");
        is("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b", "Diff");
        is("#!/usr/bin/env python3\nprint('x')", "Python");
        is("#!/bin/bash\necho hi", "Shell");
        is("SELECT id, name FROM users WHERE id = 1;", "SQL");
    }

    #[test]
    fn documents_and_styles() {
        is("# Title\n\nSome text with a [link](http://x).\n\n- one\n- two", "Markdown");
        is("body {\n  color: red;\n  margin: 0;\n}\n.a { }", "CSS");
        is("name: app\nversion: 1\nitems:\n  - a\n  - b\nport: 80", "YAML");
    }

    #[test]
    fn programming_languages() {
        is("def add(a, b):\n    return a + b\n\nif __name__ == '__main__':\n    print(add(1, 2))", "Python");
        is("fn main() {\n    let mut x = 1;\n    println!(\"{x}\");\n}", "Rust");
        is("package main\n\nimport \"fmt\"\n\nfunc main() {\n    fmt.Println(\"hi\")\n}", "Go");
        is("using System;\n\nnamespace App {\n    class P { static void Main() { Console.WriteLine(\"hi\"); } }\n}", "C#");
        is("import java.util.List;\n\npublic class A {\n    public static void main(String[] a) { System.out.println(1); }\n}", "Java");
        is("const x = require('x');\nconsole.log(x);\nconst f = () => 1;", "JavaScript");
        is("interface User { name: string; age: number; }\nexport const u: User = { name: 'a', age: 1 };", "TypeScript");
        is("#include <iostream>\n\nint main() {\n    std::cout << 1;\n}", "C++");
        is("@echo off\nset /p name=Name:\necho %name%\ngoto :eof", "Batch");
        is("param($Name)\nWrite-Host \"Hi $Name\"\nGet-ChildItem | Where-Object { $_ -eq 1 }", "PowerShell");
        is("<?php\necho $x;\n$this->run();", "PHP");
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        assert_eq!(detect_language("hello world, this is just a note to myself"), None);
        assert_eq!(detect_language("Select the item you want to buy from the shop."), None);
        assert_eq!(detect_language("short"), None);
    }
}
