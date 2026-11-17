//! A Word file to Markdown.

use std::collections::HashMap;
use std::io::Read as _;

use super::parts::{Numbering, link_targets, style_names};
use super::render::{Block, Run, inline, render, style_level};
use super::xml::{Tok, attr, tokens};
use super::{MAX_PART, bad};
use crate::error::Error;

/// The Markdown for a `.docx` file's bytes.
///
/// # Errors
/// The bytes are not a Word file (not a zip, or no `word/document.xml`).
pub fn to_markdown(bytes: &[u8]) -> Result<String, Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(bad)?;
    let mut part = |name: &str| -> Option<String> {
        let file = zip.by_name(name).ok()?;
        let mut buf = Vec::new();
        file.take(MAX_PART + 1).read_to_end(&mut buf).ok()?;
        (crate::num::to_u64(buf.len()) <= MAX_PART).then(|| String::from_utf8_lossy(&buf).into_owned())
    };
    let document = part("word/document.xml").ok_or_else(|| Error::BadArchive("this is not a Word (.docx) file: it has no document".into()))?;
    let styles = part("word/styles.xml").map(|x| style_names(&x)).unwrap_or_default();
    let numbering = part("word/numbering.xml").map(|x| Numbering::read(&x)).unwrap_or_default();
    let links = part("word/_rels/document.xml.rels").map(|x| link_targets(&x)).unwrap_or_default();
    Ok(Reader { styles, numbering, links, ..Reader::default() }.run(&tokens(&document)))
}

#[derive(Default)]
struct Para {
    heading: usize,
    quote: bool,
    code: bool,
    list: Option<(bool, usize)>,
    rule: bool,
    num: String,
    start: usize,
    runs: Vec<Run>,
}

#[derive(Default)]
struct Reader {
    styles: HashMap<String, String>,
    numbering: Numbering,
    links: HashMap<String, String>,
    blocks: Vec<Block>,
    para: Para,
    run: Run,
    link: Option<String>,
    in_ppr: bool,
    in_rpr: bool,
    in_run: bool,
    in_text: bool,
    num_id: Option<String>,
    num_level: usize,
    style_name: String,
    table_depth: usize,
    rows: Vec<Vec<String>>,
    cell: Vec<String>,
    cells: Vec<String>,
}

fn on(a: &[(String, String)]) -> bool {
    !matches!(attr(a, "val"), Some("0" | "false" | "none" | "off"))
}

impl Reader {
    fn run(mut self, toks: &[Tok]) -> String {
        for t in toks {
            match t {
                Tok::Start(n, a) => self.start(n, a),
                Tok::End(n) => self.end(n),
                Tok::Text(s) if self.in_text => self.run.text.push_str(s),
                Tok::Text(_) => {}
            }
        }
        render(&self.blocks)
    }

    fn start(&mut self, name: &str, a: &[(String, String)]) {
        match name {
            "p" => {
                self.para = Para::default();
                self.num_id = None;
                self.num_level = 0;
                self.style_name.clear();
            }
            "pPr" => self.in_ppr = true,
            "bottom" if self.in_ppr => self.para.rule = true,
            "pStyle" if self.in_ppr => {
                let id = attr(a, "val").unwrap_or_default();
                self.style_name = self.styles.get(id).cloned().unwrap_or_else(|| id.to_lowercase());
            }
            "ilvl" if self.in_ppr => self.num_level = attr(a, "val").and_then(|v| v.parse().ok()).unwrap_or(0),
            "numId" if self.in_ppr => self.num_id = attr(a, "val").map(str::to_string),
            "hyperlink" => {
                self.link = attr(a, "id").and_then(|id| self.links.get(id).cloned()).or_else(|| attr(a, "anchor").map(|s| format!("#{s}")));
            }
            "r" => {
                self.in_run = true;
                self.run = Run { link: self.link.clone(), ..Run::default() };
            }
            "rPr" if self.in_run => self.in_rpr = true,
            "b" if self.in_rpr => self.run.bold = on(a),
            "i" if self.in_rpr => self.run.italic = on(a),
            "strike" | "dstrike" if self.in_rpr => self.run.strike = on(a),
            "rFonts" if self.in_rpr => {
                let font = attr(a, "ascii").unwrap_or_default().to_lowercase();
                self.run.code = ["consolas", "courier", "mono", "menlo", "source code"].iter().any(|m| font.contains(m));
            }
            "t" if self.in_run => self.in_text = true,
            "tab" if self.in_run && !self.in_rpr => self.run.text.push(' '),
            "br" | "cr" if self.in_run => {
                if attr(a, "type").is_none_or(|t| t == "textWrapping") {
                    self.run.text.push('\n');
                }
            }
            "tbl" => {
                self.table_depth += 1;
                if self.table_depth == 1 {
                    self.rows.clear();
                }
            }
            "tr" if self.table_depth == 1 => self.cells.clear(),
            "tc" if self.table_depth == 1 => self.cell.clear(),
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        match name {
            "pPr" => {
                self.in_ppr = false;
                self.settle();
            }
            "rPr" => self.in_rpr = false,
            "t" => self.in_text = false,
            "hyperlink" => self.link = None,
            "r" => {
                self.in_run = false;
                self.in_rpr = false;
                let run = std::mem::take(&mut self.run);
                if !run.text.is_empty() {
                    self.para.runs.push(run);
                }
            }
            "p" => self.finish_para(),
            "tc" if self.table_depth == 1 => {
                let text = self.cell.join(" ").replace('|', "\\|");
                self.cells.push(text);
            }
            "tr" if self.table_depth == 1 => self.rows.push(std::mem::take(&mut self.cells)),
            "tbl" => {
                self.table_depth = self.table_depth.saturating_sub(1);
                if self.table_depth == 0 && !self.rows.is_empty() {
                    let rows = std::mem::take(&mut self.rows);
                    self.blocks.push(Block::Table(rows));
                }
            }
            _ => {}
        }
    }

    /// What the paragraph's style and numbering make it.
    fn settle(&mut self) {
        let s = self.style_name.as_str();
        if s == "title" {
            self.para.heading = 1;
        } else if let Some(n) = s.strip_prefix("heading ").and_then(|n| n.trim().parse::<usize>().ok()) {
            self.para.heading = n.clamp(1, 6);
        } else if s.contains("quote") {
            self.para.quote = true;
        } else if s.contains("code") || s.contains("preformatted") {
            self.para.code = true;
        }
        let level = self.num_level;
        match self.num_id.as_deref() {
            Some("0") => {}
            Some(id) => {
                self.para.list = Some((self.numbering.is_ordered(id, level), level));
                self.para.num = id.to_string();
                self.para.start = self.numbering.first(id, level);
            }
            None if s.starts_with("list bullet") => self.para.list = Some((false, style_level(s))),
            None if s.starts_with("list number") => self.para.list = Some((true, style_level(s))),
            None => {}
        }
    }

    fn finish_para(&mut self) {
        let para = std::mem::take(&mut self.para);
        if self.table_depth > 0 {
            let mut runs = para.runs;
            if self.rows.is_empty() {
                for r in &mut runs {
                    r.bold = false;
                }
            }
            let text = inline(&runs, false).replace('\n', " ").trim().to_string();
            if self.table_depth == 1 && !text.is_empty() {
                self.cell.push(text);
            }
            return;
        }
        if para.code {
            let text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
            self.blocks.push(Block::Code(text));
            return;
        }
        let text = inline(&para.runs, para.heading > 0);
        if text.trim().is_empty() {
            if para.rule {
                self.blocks.push(Block::Rule);
            }
            return;
        }
        self.blocks.push(if para.heading > 0 {
            Block::Heading(para.heading, text.replace('\n', " "))
        } else if let Some((ordered, level)) = para.list {
            Block::Item(ordered, level, text.replace('\n', "  \n"), para.num, para.start)
        } else if para.quote {
            Block::Quote(text)
        } else {
            Block::Text(text)
        });
    }
}
