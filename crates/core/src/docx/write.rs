//! Markdown to a Word file.

use std::io::Write as _;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::bad;
use crate::error::Error;

/// A Word document for Markdown `src`, as the bytes of a `.docx` file.
///
/// # Errors
/// The zip could not be written (it is held in memory, so this is not expected).
pub fn from_markdown(src: &str) -> Result<Vec<u8>, Error> {
    let mut w = Writer::default();
    w.run(src);
    w.finish()
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

#[derive(Clone, Copy, Default)]
struct Look {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

const TABLE_START: &str = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="999999"/><w:left w:val="single" w:sz="4" w:color="999999"/><w:bottom w:val="single" w:sz="4" w:color="999999"/><w:right w:val="single" w:sz="4" w:color="999999"/><w:insideH w:val="single" w:sz="4" w:color="999999"/><w:insideV w:val="single" w:sz="4" w:color="999999"/></w:tblBorders></w:tblPr>"#;

const MONO: &str = r#"<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>"#;

fn run(text: &str, look: Look, link: bool) -> String {
    let mut props = String::new();
    if link {
        props.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
    }
    if look.code {
        props.push_str(MONO);
    }
    if look.bold {
        props.push_str("<w:b/>");
    }
    if look.italic {
        props.push_str("<w:i/>");
    }
    if look.strike {
        props.push_str("<w:strike/>");
    }
    let rpr = if props.is_empty() { String::new() } else { format!("<w:rPr>{props}</w:rPr>") };
    text.split('\n')
        .map(|line| format!(r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#, esc(line.trim_end_matches('\r'))))
        .collect::<Vec<_>>()
        .join("<w:r><w:br/></w:r>")
}

#[derive(Default)]
struct Para {
    style: Option<String>,
    num: Option<(usize, usize)>,
    rule: bool,
    align: Option<&'static str>,
    runs: String,
}

impl Para {
    fn xml(&self) -> String {
        let mut p = String::new();
        if let Some(s) = &self.style {
            p.push_str(&format!(r#"<w:pStyle w:val="{s}"/>"#));
        }
        if let Some((id, lvl)) = self.num {
            p.push_str(&format!(r#"<w:numPr><w:ilvl w:val="{lvl}"/><w:numId w:val="{id}"/></w:numPr>"#));
        }
        if self.rule {
            p.push_str(r#"<w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="999999"/></w:pBdr>"#);
        }
        if let Some(a) = self.align {
            p.push_str(&format!(r#"<w:jc w:val="{a}"/>"#));
        }
        format!("<w:p><w:pPr>{p}</w:pPr>{}</w:p>", self.runs)
    }
}

#[derive(Default)]
struct Writer {
    body: String,
    cell: String,
    in_cell: bool,
    para: Option<Para>,
    look: Look,
    link: Option<usize>,
    links: Vec<String>,
    /// For each open list: its number id (0 for a bullet list).
    lists: Vec<usize>,
    /// The start of each numbered list, by number id - 2.
    starts: Vec<u64>,
    item_first: bool,
    quotes: usize,
    code: Option<String>,
    aligns: Vec<pulldown_cmark::Alignment>,
    column: usize,
    header: bool,
}

impl Writer {
    fn emit(&mut self, xml: &str) {
        if self.in_cell { &mut self.cell } else { &mut self.body }.push_str(xml);
    }

    fn open(&mut self) -> &mut Para {
        if self.para.is_none() {
            let mut p = Para::default();
            if !self.lists.is_empty() && self.item_first {
                self.item_first = false;
                p.num = Some((self.lists.last().map_or(1, |n| if *n == 0 { 1 } else { *n }), self.lists.len() - 1));
            } else if self.quotes > 0 {
                p.style = Some("Quote".into());
            }
            if self.in_cell {
                p.align = self.aligns.get(self.column).and_then(|a| match a {
                    pulldown_cmark::Alignment::Center => Some("center"),
                    pulldown_cmark::Alignment::Right => Some("right"),
                    _ => None,
                });
            }
            self.para = Some(p);
        }
        self.para.as_mut().unwrap_or_else(|| unreachable!())
    }

    fn flush(&mut self) {
        if let Some(p) = self.para.take() {
            let xml = p.xml();
            self.emit(&xml);
        }
    }

    fn text(&mut self, s: &str) {
        if let Some(code) = &mut self.code {
            code.push_str(s);
            return;
        }
        let look = Look { bold: self.look.bold || self.header, ..self.look };
        let xml = run(s, look, self.link.is_some());
        let link = self.link.map(|i| i + 10);
        let p = self.open();
        match link {
            Some(id) => p.runs.push_str(&format!(r#"<w:hyperlink r:id="rId{id}" w:history="1">{xml}</w:hyperlink>"#)),
            None => p.runs.push_str(&xml),
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.para = Some(Para { style: Some(format!("Heading{}", level as usize)), ..Para::default() });
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quotes += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(first) => {
                self.flush();
                let id = match first {
                    Some(start) => {
                        self.starts.push(start);
                        self.starts.len() + 1
                    }
                    None => 0,
                };
                self.lists.push(id);
            }
            Tag::Item => {
                self.flush();
                self.item_first = true;
            }
            Tag::Emphasis => self.look.italic = true,
            Tag::Strong => self.look.bold = true,
            Tag::Strikethrough => self.look.strike = true,
            Tag::Link { dest_url, .. } => {
                self.links.push(dest_url.to_string());
                self.link = Some(self.links.len() - 1);
            }
            Tag::Table(aligns) => {
                self.flush();
                self.aligns = aligns;
                self.emit(TABLE_START);
            }
            Tag::TableHead => {
                self.header = true;
                self.column = 0;
                self.emit("<w:tr>");
            }
            Tag::TableRow => {
                self.column = 0;
                self.emit("<w:tr>");
            }
            Tag::TableCell => {
                self.in_cell = true;
                self.cell.clear();
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item => self.flush(),
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quotes = self.quotes.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                for line in self.code.take().unwrap_or_default().trim_end_matches('\n').split('\n') {
                    let p = Para { style: Some("Code".into()), runs: run(line, Look { code: true, ..Look::default() }, false), ..Para::default() };
                    self.emit(&p.xml());
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Emphasis => self.look.italic = false,
            TagEnd::Strong => self.look.bold = false,
            TagEnd::Strikethrough => self.look.strike = false,
            TagEnd::Link => self.link = None,
            TagEnd::TableCell => {
                self.flush();
                let mut cell = std::mem::take(&mut self.cell);
                if cell.is_empty() {
                    cell.push_str("<w:p/>");
                }
                self.in_cell = false;
                self.column += 1;
                self.emit(&format!(r#"<w:tc><w:tcPr><w:tcW w:w="0" w:type="auto"/></w:tcPr>{cell}</w:tc>"#));
            }
            TagEnd::TableHead => {
                self.emit("</w:tr>");
                self.header = false;
            }
            TagEnd::TableRow => self.emit("</w:tr>"),
            TagEnd::Table => self.emit("</w:tbl><w:p/>"),
            _ => {}
        }
    }

    fn run(&mut self, src: &str) {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        for event in Parser::new_ext(src, options) {
            match event {
                Event::Start(tag) => self.start(tag),
                Event::End(tag) => self.end(tag),
                Event::Code(t) => {
                    let saved = self.look;
                    self.look.code = true;
                    self.text(&t);
                    self.look = saved;
                }
                Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => self.text(&t),
                Event::SoftBreak => self.text(" "),
                Event::HardBreak => self.text("\n"),
                Event::Rule => {
                    self.flush();
                    self.emit(&Para { rule: true, ..Para::default() }.xml());
                }
                Event::TaskListMarker(done) => self.text(if done { "\u{2612} " } else { "\u{2610} " }),
                _ => {}
            }
        }
        self.flush();
    }

    fn numbering(&self) -> String {
        let level = |bullet: bool, lvl: usize| {
            let (fmt, text) = if bullet { ("bullet".to_string(), "\u{2022}".to_string()) } else { ("decimal".to_string(), format!("%{}.", lvl + 1)) };
            format!(
                r#"<w:lvl w:ilvl="{lvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{}" w:hanging="360"/></w:pPr></w:lvl>"#,
                720 * (lvl + 1)
            )
        };
        let abstract_num = |id: usize, bullet: bool| {
            format!(
                r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="hybridMultilevel"/>{}</w:abstractNum>"#,
                (0..9).map(|l| level(bullet, l)).collect::<String>()
            )
        };
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        );
        out.push_str(&abstract_num(0, true));
        out.push_str(&abstract_num(1, false));
        out.push_str(r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#);
        for (i, start) in self.starts.iter().enumerate() {
            let mut overrides = String::new();
            for l in 0..9 {
                overrides.push_str(&format!(r#"<w:lvlOverride w:ilvl="{l}"><w:startOverride w:val="{}"/></w:lvlOverride>"#, if l == 0 { *start } else { 1 }));
            }
            out.push_str(&format!(r#"<w:num w:numId="{}"><w:abstractNumId w:val="1"/>{overrides}</w:num>"#, i + 2));
        }
        out.push_str("</w:numbering>");
        out
    }

    fn finish(self) -> Result<Vec<u8>, Error> {
        const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
            self.body
        );
        let mut rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
        );
        for (i, url) in self.links.iter().enumerate() {
            rels.push_str(&format!(
                r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>"#,
                i + 10,
                esc(url)
            ));
        }
        rels.push_str("</Relationships>");
        let parts: [(&str, String); 6] = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#.into(),
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.into(),
            ),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", rels),
            ("word/styles.xml", styles()),
            ("word/numbering.xml", self.numbering()),
        ];
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, content) in parts {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).map_err(bad)?;
            zip.write_all(content.as_bytes()).map_err(bad)?;
        }
        Ok(zip.finish().map_err(bad)?.into_inner())
    }
}

fn styles() -> String {
    let heading = |n: usize, size: usize| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{n}"><w:name w:val="heading {n}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/></w:rPr></w:style>"#,
            n - 1
        )
    };
    let headings: String = [(1, 40), (2, 32), (3, 28), (4, 24), (5, 22), (6, 22)].iter().map(|(n, s)| heading(*n, *s)).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="264" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>{headings}<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:pBdr><w:left w:val="single" w:sz="18" w:space="8" w:color="BBBBBB"/></w:pBdr><w:ind w:left="567"/></w:pPr><w:rPr><w:i/><w:color w:val="555555"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="F2F2F2"/><w:spacing w:after="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/><w:sz w:val="20"/></w:rPr></w:style><w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style></w:styles>"#
    )
}
