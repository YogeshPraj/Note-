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
    /// A JSON document: shown in the tree/code/text JSON editor.
    Json,
    /// An `.http` / `.rest` scratchpad: the preview pane shows the response of the last request.
    Http,
    /// A `.log` / `.out` / `.err` file: the pane follows it as it grows (Log Follow).
    Log,
}

/// What kind of preview (if any) a document supports.
pub fn kind_for(lexer: &str, path: Option<&Path>) -> Option<Kind> {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("mmd" | "mermaid") => return Some(Kind::Mermaid),
        Some("http" | "rest") => return Some(Kind::Http),
        Some("log" | "out" | "err") => return Some(Kind::Log),
        _ => {}
    }
    match lexer {
        "markdown" => Some(Kind::Markdown),
        "hypertext" => Some(Kind::Html),
        "json" => Some(Kind::Json),
        _ => None,
    }
}

const DIAGRAMS: [&str; 20] = [
    "graph",
    "flowchart",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "gitGraph",
    "mindmap",
    "timeline",
    "quadrantChart",
    "requirementDiagram",
    "C4Context",
    "sankey",
    "xychart",
    "block",
    "architecture",
    "zenuml",
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
/// `mermaid` code fences become `<pre class="mermaid">` for the page to draw.
///
/// Every top-level block is preceded by an empty `<span class="ln" data-line="N">` (N = the
/// 0-based source line it starts on). The preview page uses these to keep its scroll position
/// in step with the editor.
pub fn markdown_to_html(src: &str) -> String {
    markdown_to_html_in(src, None)
}

/// Like `markdown_to_html`, and with `base` (the folder of the document) images that point at
/// local files are embedded, so they show although the page itself may load no files.
pub fn markdown_to_html_in(src: &str, base: Option<&Path>) -> String {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH;
    let line_starts: Vec<usize> = std::iter::once(0).chain(src.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset) - 1;
    let marker = |offset: usize| Event::Html(format!("<span class=\"ln\" data-line=\"{}\"></span>\n", line_of(offset)).into());

    let mut events: Vec<Event> = Vec::new();
    let mut mermaid: Option<String> = None;
    let mut headings = Headings::default();
    let mut depth = 0usize;
    for (ev, range) in Parser::new_ext(src, opts).into_offset_iter() {
        headings.read(&ev);
        // Mark where each top-level block starts (nested items are not marked).
        match &ev {
            Event::Start(_) => {
                if depth == 0 {
                    events.push(marker(range.start));
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule | Event::Html(_) if depth == 0 => events.push(marker(range.start)),
            _ => {}
        }
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) if lang.split_whitespace().next() == Some("mermaid") => {
                mermaid = Some(String::new());
            }
            Event::Text(t) if mermaid.is_some() => mermaid.iter_mut().for_each(|m| m.push_str(&t)),
            Event::End(TagEnd::CodeBlock) if mermaid.is_some() => {
                let code = mermaid.take().unwrap_or_default();
                events.push(Event::Html(format!("<pre class=\"mermaid\">{}</pre>\n", escape(&code)).into()));
            }
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                let dest = base.and_then(|b| local_image_data_uri(b, &dest_url)).map(Into::into).unwrap_or(dest_url);
                events.push(Event::Start(Tag::Image { link_type, dest_url: dest, title, id }));
            }
            // Raw HTML is displayed, not executed.
            Event::Html(s) | Event::InlineHtml(s) => events.push(Event::Text(s)),
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let bad = !safe_link(&dest_url);
                let dest = if bad { "#".into() } else { dest_url };
                events.push(Event::Start(Tag::Link { link_type: if bad { LinkType::Inline } else { link_type }, dest_url: dest, title, id }));
            }
            Event::Start(Tag::Heading { .. }) => {
                headings.open(events.len());
                events.push(ev);
            }
            Event::End(TagEnd::Heading(_)) => {
                headings.close(&mut events);
                events.push(ev);
            }
            other => events.push(other),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

/// Gives every heading an `id` (GitHub's rules: lower case, spaces to hyphens, punctuation dropped, `-1`, `-2` for
/// repeats) so links such as `#install` reach it. A heading with an id of its own keeps it.
#[derive(Default)]
struct Headings {
    open: Option<(usize, String)>,
    seen: std::collections::HashMap<String, usize>,
}

impl Headings {
    fn open(&mut self, at: usize) {
        self.open = Some((at, String::new()));
    }

    /// Collect the text of the heading being read.
    fn read(&mut self, ev: &Event) {
        if let (Some((_, text)), Event::Text(t) | Event::Code(t)) = (self.open.as_mut(), ev) {
            text.push_str(t);
        }
    }

    fn close(&mut self, events: &mut [Event]) {
        let Some((at, text)) = self.open.take() else { return };
        let slug = slug(&text);
        let n = self.seen.entry(slug.clone()).or_insert(0);
        let id = if *n == 0 { slug } else { format!("{slug}-{n}") };
        *n += 1;
        if let Some(Event::Start(Tag::Heading { id: own, .. })) = events.get_mut(at)
            && own.is_none()
        {
            *own = Some(id.into());
        }
    }
}

fn slug(text: &str) -> String {
    let s: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect();
    if s.is_empty() { "section".into() } else { s }
}

/// Biggest local image that is embedded in the preview.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

/// A `data:` URI for a relative (or drive-letter) image path next to the document, if it is a
/// picture of a known type and not too big.
fn local_image_data_uri(base: &Path, dest: &str) -> Option<String> {
    let d = dest.trim();
    // Anything with a scheme (http:, data:, file:...) or a network path is not a local file here.
    let drive = d.len() > 2 && d.as_bytes()[1] == b':' && matches!(d.as_bytes()[2], b'\\' | b'/');
    if d.is_empty() || d.starts_with("//") || d.starts_with('#') || (d.contains(':') && !drive) {
        return None;
    }
    let d = d.split(['?', '#']).next().unwrap_or(d);
    let path = base.join(super::textools::url_decode(d).replace('/', std::path::MAIN_SEPARATOR_STR));
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => return None,
    };
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    Some(format!("data:{mime};base64,{}", super::textools::base64_encode(&bytes)))
}

/// Links may only go to http(s), mailto, or stay on the page (no scheme at all).
/// The address of a link clicked in the preview, when it may go to the system's browser or mail program.
pub fn external_link(url: &str) -> Option<&str> {
    let u = url.trim();
    let (scheme, _) = u.split_once(':')?;
    (["http", "https", "mailto"].iter().any(|s| s.eq_ignore_ascii_case(scheme)) && !u.chars().any(char::is_control)).then_some(u)
}

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
mod tests;
