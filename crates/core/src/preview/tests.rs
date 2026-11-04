use super::*;

#[test]
fn local_images_are_embedded() {
    let d = std::env::temp_dir().join(format!("npp_md_img_{}", std::process::id()));
    let _ = std::fs::create_dir_all(d.join("img"));
    std::fs::write(d.join("img").join("a b.png"), [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::write(d.join("notes.txt"), "secret").unwrap();
    let html = markdown_to_html_in("![x](img/a%20b.png) ![y](notes.txt) ![z](https://e.com/a.png) ![w](missing.png)", Some(&d));
    assert!(html.contains("src=\"data:image/png;base64,iVBORw==\""), "{html}");
    // Not an image type, a web address, or absent: left exactly as written.
    assert!(html.contains("src=\"notes.txt\"") && html.contains("src=\"https://e.com/a.png\"") && html.contains("src=\"missing.png\""));
    // Without a base folder nothing is read.
    assert!(markdown_to_html("![x](img/a%20b.png)").contains("src=\"img/a%20b.png\""));
    let _ = std::fs::remove_dir_all(&d);
}
#[test]
fn kinds() {
    assert_eq!(kind_for("markdown", None), Some(Kind::Markdown));
    assert_eq!(kind_for("hypertext", None), Some(Kind::Html));
    assert_eq!(kind_for("json", None), Some(Kind::Json));
    assert_eq!(kind_for("null", Some(Path::new("api/calls.HTTP"))), Some(Kind::Http));
    assert_eq!(kind_for("null", Some(Path::new("x.rest"))), Some(Kind::Http));
    assert_eq!(kind_for("null", Some(Path::new("server.LOG"))), Some(Kind::Log));
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
fn blocks_carry_their_source_line() {
    let h = markdown_to_html("# A\n\npara one\nstill para\n\n- x\n- y\n\n```mermaid\ngraph TD\n```\n\n---\n");
    let lines: Vec<usize> = h.split("data-line=\"").skip(1).map(|s| s.split('"').next().unwrap().parse().unwrap()).collect();
    // heading, paragraph, list (items are not marked), mermaid fence, rule
    assert_eq!(lines, vec![0, 2, 5, 8, 12], "{h}");
    assert!(h.contains("<span class=\"ln\" data-line=\"0\"></span>\n<h1 id=\"a\">A</h1>"), "{h}");
}

#[test]
fn markers_survive_crlf_and_html_blocks() {
    let h = markdown_to_html("a\r\n\r\n<div>x</div>\r\n\r\nb");
    let lines: Vec<usize> = h.split("data-line=\"").skip(1).map(|s| s.split('"').next().unwrap().parse().unwrap()).collect();
    assert_eq!(lines, vec![0, 2, 4], "{h}");
}

#[test]
fn headings_get_github_style_ids() {
    let h = markdown_to_html("# Hello, World!\n\n## Install `cargo`\n\n## Install `cargo`\n\n### \n");
    assert!(h.contains("<h1 id=\"hello-world\">"), "{h}");
    assert!(h.contains("<h2 id=\"install-cargo\">") && h.contains("<h2 id=\"install-cargo-1\">"), "{h}");
    assert_eq!(slug("  Ünïcode Ñame "), "ünïcode-ñame");
    assert_eq!(slug("!!!"), "section");
}

#[test]
fn github_alerts_become_classed_blocks() {
    let h = markdown_to_html("> [!WARNING]\n> Careful here.\n\n> plain quote\n");
    assert!(h.contains("markdown-alert-warning"), "{h}");
    assert!(h.contains("Careful here."), "{h}");
    assert!(!h.contains("[!WARNING]"), "{h}");
    assert_eq!(h.matches("<blockquote").count(), 2);
}

#[test]
fn math_is_marked_for_the_page_to_typeset() {
    let h = markdown_to_html("Inline $a<b$ and\n\n$$\\frac{1}{2}$$\n\nPrice is $5 or $10.");
    assert!(h.contains("<span class=\"math math-inline\">a&lt;b</span>"), "{h}");
    assert!(h.contains("math-display") && h.contains("\\frac{1}{2}"), "{h}");
    assert_eq!(h.matches("class=\"math ").count(), 2, "currency is not math: {h}");
}

#[test]
fn renders_gfm() {
    let h = markdown_to_html("# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~gone~~");
    assert!(h.contains("<h1 id=\"hi\">Hi</h1>"));
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
        // The braces stay visible as text and never become attributes: the only attribute is the generated id,
        // which holds nothing but letters, digits, hyphens and underscores.
        for tag in h.split("<h").skip(1).map(|s| s.split('>').next().unwrap_or("")) {
            let attrs = tag.get(1..).unwrap_or("");
            assert!(
                attrs.is_empty()
                    || attrs
                        .strip_prefix(" id=\"")
                        .and_then(|r| r.strip_suffix('"'))
                        .is_some_and(|id| id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')),
                "{h}"
            );
        }
    }
}

#[test]
fn only_web_and_mail_links_leave_the_preview() {
    assert_eq!(external_link(" https://a.b/c?d=1&e=2 "), Some("https://a.b/c?d=1&e=2"));
    assert_eq!(external_link("HTTP://a.b"), Some("HTTP://a.b"));
    assert_eq!(external_link("mailto:a@b.c"), Some("mailto:a@b.c"));
    for bad in ["javascript:alert(1)", "file:///C:/x.exe", "ms-msdt:/id", "search-ms:q", "docs/readme.md", "", "https://a\nb"] {
        assert_eq!(external_link(bad), None, "{bad}");
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
