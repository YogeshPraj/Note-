//! Live-preview content: which files can be previewed, and turning Markdown
//! into HTML. Pure and portable; the platform shell only displays the result.
//!
//! Safety: raw HTML inside Markdown is shown as text (never injected) and
//! `javascript:` links are neutralised, so a hostile `.md` cannot run script in
//! the preview page.

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd, html};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Markdown,
    Html,
    Mermaid,
}

/// What kind of preview (if any) a document supports.
pub fn kind_for(lexer: &str, path: Option<&Path>) -> Option<Kind> {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("mmd" | "mermaid") => return Some(Kind::Mermaid),
        _ => {}
    }
    match lexer {
        "markdown" => Some(Kind::Markdown),
        "hypertext" => Some(Kind::Html),
        _ => None,
    }
}

const DIAGRAMS: [&str; 20] = [
    "graph", "flowchart", "sequenceDiagram", "classDiagram", "stateDiagram", "erDiagram", "journey", "gantt", "pie", "gitGraph",
    "mindmap", "timeline", "quadrantChart", "requirementDiagram", "C4Context", "sankey", "xychart", "block", "architecture", "zenuml",
];

/// A file that is nothing but a Mermaid diagram (no fences), e.g. `graph TD; A-->B`.
pub fn is_bare_mermaid(text: &str) -> bool {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let mut first = lines.next();
    if first == Some("---") {
        // Skip a YAML front-matter block.
        for l in lines.by_ref() {
            if l == "---" {
                break;
            }
        }
        first = lines.next();
    }
    let Some(line) = first else { return false };
    DIAGRAMS.iter().any(|d| line.strip_prefix(d).is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric())))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Render Markdown (GFM tables, task lists, strikethrough, footnotes) to HTML.
/// ```mermaid fences become `<pre class="mermaid">` for the page to draw.
pub fn markdown_to_html(src: &str) -> String {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let mut events: Vec<Event> = Vec::new();
    let mut mermaid: Option<String> = None;
    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) if lang.split_whitespace().next() == Some("mermaid") => {
                mermaid = Some(String::new());
            }
            Event::Text(t) if mermaid.is_some() => mermaid.as_mut().unwrap().push_str(&t),
            Event::End(TagEnd::CodeBlock) if mermaid.is_some() => {
                let code = mermaid.take().unwrap();
                events.push(Event::Html(format!("<pre class=\"mermaid\">{}</pre>\n", escape(&code)).into()));
            }
            // Raw HTML is displayed, not executed.
            Event::Html(s) | Event::InlineHtml(s) => events.push(Event::Text(s)),
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let bad = !safe_link(&dest_url);
                let dest = if bad { "#".into() } else { dest_url };
                events.push(Event::Start(Tag::Link { link_type: if bad { LinkType::Inline } else { link_type }, dest_url: dest, title, id }));
            }
            other => events.push(other),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

/// Links may only go to http(s), mailto, or stay on the page (no scheme at all).
fn safe_link(url: &str) -> bool {
    let u = url.trim_start();
    match u.find([':', '/', '?', '#']) {
        Some(i) if u.as_bytes()[i] == b':' => {
            let scheme = u[..i].to_ascii_lowercase();
            matches!(scheme.as_str(), "http" | "https" | "mailto")
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind_for("markdown", None), Some(Kind::Markdown));
        assert_eq!(kind_for("hypertext", None), Some(Kind::Html));
        assert_eq!(kind_for("null", Some(Path::new("x/diagram.MMD"))), Some(Kind::Mermaid));
        assert_eq!(kind_for("rust", Some(Path::new("a.rs"))), None);
    }

    #[test]
    fn detects_bare_mermaid() {
        assert!(is_bare_mermaid("graph TD\n  A-->B"));
        assert!(is_bare_mermaid("\n%% comment\nflowchart LR\nA-->B"));
        assert!(is_bare_mermaid("---\ntitle: x\n---\nsequenceDiagram\nA->>B: hi"));
        assert!(!is_bare_mermaid("# Title\n\ngraph is a word"));
        assert!(!is_bare_mermaid("graphite is not a diagram"));
        assert!(!is_bare_mermaid(""));
    }

    #[test]
    fn renders_gfm() {
        let h = markdown_to_html("# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~gone~~");
        assert!(h.contains("<h1>Hi</h1>"));
        assert!(h.contains("<table>"));
        assert!(h.contains("checked"));
        assert!(h.contains("<del>gone</del>"));
    }

    #[test]
    fn mermaid_fence_becomes_diagram_node() {
        let h = markdown_to_html("```mermaid\ngraph TD\n  A-->B\n```\n");
        assert!(h.contains("<pre class=\"mermaid\">graph TD\n  A--&gt;B\n</pre>"), "{h}");
        let h = markdown_to_html("```rust\nlet a = 1;\n```\n");
        assert!(h.contains("language-rust") && !h.contains("class=\"mermaid\""));
    }

    #[test]
    fn raw_html_and_js_links_are_neutralised() {
        let h = markdown_to_html("<script>alert(1)</script>\n\nhi <img src=x onerror=alert(1)>\n\n[x](javascript:alert(1))");
        assert!(!h.contains("<script>") && !h.contains("<img"), "{h}");
        assert!(h.contains("&lt;script&gt;"));
        assert!(!h.to_lowercase().contains("javascript:"), "{h}");
    }

    #[test]
    fn heading_attributes_cannot_carry_event_handlers() {
        for src in ["# Hi {onmouseover=alert(1) #x .c}", "setext {onclick=alert(2)}\n===", "## T {#id onfocus=x}"] {
            let h = markdown_to_html(src).to_lowercase();
            // Plain `<h1>` / `<h2>` only: the braces stay visible as text, never become attributes.
            assert!(!h.contains("<h1 ") && !h.contains("<h2 "), "{h}");
        }
    }

    #[test]
    fn only_safe_link_schemes_survive() {
        for bad in ["javascript:alert(1)", "  JaVaScRiPt:x", "vbscript:x", "file://evil/share/x", "data:text/html,<b>", "ms-msdt:/id"] {
            let h = markdown_to_html(&format!("[x]({bad})"));
            assert!(h.contains("href=\"#\""), "{bad}: {h}");
        }
        for ok in ["https://a.b/c", "http://a.b", "mailto:a@b.c", "#frag", "docs/readme.md", "./x:y"] {
            let h = markdown_to_html(&format!("[x]({ok})"));
            assert!(!h.contains("href=\"#\"") || ok == "#frag", "{ok}: {h}");
        }
    }
}
