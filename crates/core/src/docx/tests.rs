use super::*;

const SAMPLE: &str = "# Title\n\nSome **bold**, *italic*, ~~gone~~ and `code` with a [link](https://example.com/a?b=1&c=2).\n\n- one\n- two\n  - nested\n\n3. three\n4. four\n\n> quoted\n\n```\nlet a = 1;\nlet b = 2;\n```\n\n| A | B |\n| --- | --- |\n| 1 | x \\| y |\n\n---\n\nLast & <tag>.\n";

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(name).unwrap(), &mut s).unwrap();
    s
}

#[test]
fn the_file_has_the_parts_word_needs_and_well_formed_xml() {
    let bytes = from_markdown(SAMPLE).unwrap();
    for name in ["[Content_Types].xml", "_rels/.rels", "word/document.xml", "word/styles.xml", "word/numbering.xml", "word/_rels/document.xml.rels"] {
        let xml = part(&bytes, name);
        let toks = xml::tokens(&xml);
        let mut depth = 0i32;
        for t in &toks {
            match t {
                xml::Tok::Start(..) => depth += 1,
                xml::Tok::End(_) => depth -= 1,
                xml::Tok::Text(_) => {}
            }
            assert!(depth >= 0, "{name} closes too much");
        }
        assert_eq!(depth, 0, "{name} leaves a tag open");
    }
    let doc = part(&bytes, "word/document.xml");
    assert!(doc.contains("Heading1") && doc.contains("<w:tbl>") && doc.contains("<w:numPr>") && doc.contains("&lt;tag&gt;"));
    assert!(part(&bytes, "word/_rels/document.xml.rels").contains("https://example.com/a?b=1&amp;c=2"));
}

#[test]
fn markdown_survives_a_round_trip() {
    let back = to_markdown(&from_markdown(SAMPLE).unwrap()).unwrap();
    for want in [
        "# Title",
        "**bold**",
        "*italic*",
        "~~gone~~",
        "`code`",
        "[link](https://example.com/a?b=1&c=2)",
        "- one\n- two\n   - nested\n\n3. three",
        "3. three\n4. four",
        "> quoted",
        "```\nlet a = 1;\nlet b = 2;\n```",
        "| A | B |",
        "\n---\n",
        "\n---\n",
        "| 1 | x \\| y |",
        "Last & \\<tag\\>.",
    ] {
        assert!(back.contains(want), "missing {want:?} in:\n{back}");
    }
}

#[test]
fn numbered_lists_restart_at_their_own_start() {
    let back = to_markdown(&from_markdown("1. a\n2. b\n\ntext\n\n1. c\n").unwrap()).unwrap();
    assert!(back.contains("1. a\n2. b"), "{back}");
    assert!(back.contains("1. c"), "{back}");
}

#[test]
fn bytes_that_are_not_a_word_file_are_refused() {
    assert!(to_markdown(b"just text").is_err());
    let empty = zip::ZipWriter::new(std::io::Cursor::new(Vec::new())).finish().unwrap().into_inner();
    assert!(to_markdown(&empty).is_err());
}

#[test]
fn a_file_made_by_word_is_read_by_its_styles_and_numbering() {
    let doc = r#"<w:document xmlns:w="w" xmlns:r="r"><w:body>
<w:p><w:pPr><w:pStyle w:val="Titre1"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Bold </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>text</w:t></w:r><w:r><w:t xml:space="preserve"> and </w:t></w:r><w:hyperlink r:id="rId7"><w:r><w:t>site</w:t></w:r></w:hyperlink></w:p>
<w:p><w:pPr><w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="5"/></w:numPr></w:pPr><w:r><w:t>first</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="6"/></w:numPr></w:pPr><w:r><w:t>dot</w:t></w:r></w:p>
<w:p><w:r><w:t>1. not a list &amp; *not bold*</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>H</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>v</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:body></w:document>"#;
    let styles = r#"<w:styles xmlns:w="w"><w:style w:styleId="Titre1"><w:name w:val="heading 1"/></w:style></w:styles>"#;
    let numbering = r#"<w:numbering xmlns:w="w"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="5"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="6"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
    let rels = r#"<Relationships><Relationship Id="rId7" Type="x" Target="https://example.org/" TargetMode="External"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in [("word/document.xml", doc), ("word/styles.xml", styles), ("word/numbering.xml", numbering), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut zip, body.as_bytes()).unwrap();
    }
    let md = to_markdown(&zip.finish().unwrap().into_inner()).unwrap();
    assert_eq!(
        md,
        "# Hello\n\n**Bold text** and [site](https://example.org/)\n\n1. first\n\n- dot\n\n\\1. not a list & \\*not bold\\*\n\n| H |\n| --- |\n| v |\n"
    );
}

#[test]
fn an_empty_document_is_an_empty_file() {
    assert_eq!(to_markdown(&from_markdown("").unwrap()).unwrap(), "\n");
}
