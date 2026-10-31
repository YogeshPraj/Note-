//! Compare: a side-by-side text diff as a self-contained HTML page.
//!
//! The page has no external resources and no script that reads the compared text; the text is
//! only ever placed in the page as escaped HTML. Long runs of unchanged lines are folded.

use similar::{ChangeTag, DiffOp, TextDiff};

/// Unchanged lines kept around each change.
const CONTEXT: usize = 3;
/// Beyond this many lines a side is not compared (the page would be unusable).
pub const MAX_LINES: usize = 60_000;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\t' => o.push_str("    "),
            c => o.push(c),
        }
    }
    o
}

fn strip_eol(l: &str) -> &str {
    l.strip_suffix("\r\n").or_else(|| l.strip_suffix('\n')).or_else(|| l.strip_suffix('\r')).unwrap_or(l)
}

/// Mark the characters that differ between two similar lines: (left html, right html).
fn inline(a: &str, b: &str) -> (String, String) {
    // Very long lines are marked as a whole; a character diff of them is slow and unreadable.
    if a.len() > 2000 || b.len() > 2000 {
        return (format!("<b>{}</b>", esc(a)), format!("<b>{}</b>", esc(b)));
    }
    let diff = TextDiff::from_chars(a, b);
    let (mut l, mut r) = (String::new(), String::new());
    // Runs of changed characters share one <b>.
    let (mut in_l, mut in_r) = (false, false);
    let mut open = |s: &mut String, state: &mut bool, want: bool| {
        if *state != want {
            s.push_str(if want { "<b>" } else { "</b>" });
            *state = want;
        }
    };
    for ch in diff.iter_all_changes() {
        let t = esc(ch.value());
        match ch.tag() {
            ChangeTag::Equal => {
                open(&mut l, &mut in_l, false);
                open(&mut r, &mut in_r, false);
                l.push_str(&t);
                r.push_str(&t);
            }
            ChangeTag::Delete => {
                open(&mut l, &mut in_l, true);
                l.push_str(&t);
            }
            ChangeTag::Insert => {
                open(&mut r, &mut in_r, true);
                r.push_str(&t);
            }
        }
    }
    open(&mut l, &mut in_l, false);
    open(&mut r, &mut in_r, false);
    (l, r)
}

struct Row {
    left: Option<(usize, String)>,
    right: Option<(usize, String)>,
    kind: &'static str, // "eq" | "del" | "ins" | "chg"
}

/// Differences found: (rows, changed blocks, lines removed, lines added).
fn rows(a: &str, b: &str) -> (Vec<Row>, usize, usize, usize) {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.split_inclusive('\n').collect(), b.split_inclusive('\n').collect());
    let diff = TextDiff::from_lines(a, b);
    let (mut out, mut blocks, mut removed, mut added) = (Vec::new(), 0, 0, 0);
    for op in diff.ops() {
        match *op {
            DiffOp::Equal { old_index, new_index, len } => {
                for k in 0..len {
                    out.push(Row {
                        left: Some((old_index + k + 1, esc(strip_eol(la[old_index + k])))),
                        right: Some((new_index + k + 1, esc(strip_eol(lb[new_index + k])))),
                        kind: "eq",
                    });
                }
            }
            DiffOp::Delete { old_index, old_len, .. } => {
                blocks += 1;
                removed += old_len;
                for k in 0..old_len {
                    out.push(Row { left: Some((old_index + k + 1, esc(strip_eol(la[old_index + k])))), right: None, kind: "del" });
                }
            }
            DiffOp::Insert { new_index, new_len, .. } => {
                blocks += 1;
                added += new_len;
                for k in 0..new_len {
                    out.push(Row { left: None, right: Some((new_index + k + 1, esc(strip_eol(lb[new_index + k])))), kind: "ins" });
                }
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                blocks += 1;
                removed += old_len;
                added += new_len;
                for k in 0..old_len.max(new_len) {
                    let l = (k < old_len).then(|| strip_eol(la[old_index + k]));
                    let r = (k < new_len).then(|| strip_eol(lb[new_index + k]));
                    let (lh, rh) = match (l, r) {
                        (Some(l), Some(r)) => {
                            let (a, b) = inline(l, r);
                            (Some(a), Some(b))
                        }
                        (Some(l), None) => (Some(esc(l)), None),
                        (None, Some(r)) => (None, Some(esc(r))),
                        (None, None) => (None, None),
                    };
                    out.push(Row {
                        left: lh.map(|h| (old_index + k + 1, h)),
                        right: rh.map(|h| (new_index + k + 1, h)),
                        kind: if l.is_some() && r.is_some() { "chg" } else if l.is_some() { "del" } else { "ins" },
                    });
                }
            }
        }
    }
    (out, blocks, removed, added)
}

fn cell(side: &Option<(usize, String)>, class: &str) -> String {
    match side {
        Some((n, h)) => format!("<td class=\"n {class}\">{n}</td><td class=\"t {class}\">{h}</td>"),
        None => "<td class=\"n void\"></td><td class=\"t void\"></td>".to_string(),
    }
}

const CSS: &str = "\
:root{--bg:#fff;--fg:#24292f;--muted:#6e7781;--bar:#f0f0f0;--line:#d8dee4;--del:#ffebe9;--delb:#ff8182;--ins:#e6ffec;--insb:#4ac26b;--chg:#fff8c5;--void:#f6f8fa}\
body.dark{--bg:#1e1e1e;--fg:#d4d4d4;--muted:#9da5ae;--bar:#2d2d2d;--line:#3c3c3c;--del:#4b1f23;--delb:#a1282f;--ins:#1c3d27;--insb:#2f8f4e;--chg:#4a3f12;--void:#252526}\
*{box-sizing:border-box}html,body{margin:0;background:var(--bg);color:var(--fg);font:13px/1.45 Consolas,'Cascadia Code',monospace}\
#bar{position:sticky;top:0;z-index:3;background:var(--bar);border-bottom:1px solid var(--line);padding:6px 12px;display:flex;gap:16px;align-items:center;font:12px 'Segoe UI',sans-serif}\
#bar .stat{color:var(--muted)}#bar .del{color:#cf222e}#bar .ins{color:#1a7f37}body.dark #bar .del{color:#ff7b72}body.dark #bar .ins{color:#56d364}\
#bar button{font:12px 'Segoe UI',sans-serif;padding:2px 10px;border:1px solid var(--line);background:var(--bg);color:var(--fg);border-radius:3px;cursor:pointer}\
table{border-collapse:collapse;width:100%;table-layout:fixed}td{padding:0 8px;vertical-align:top;white-space:pre-wrap;word-break:break-all}\
td.n{width:52px;text-align:right;color:var(--muted);user-select:none;border-right:1px solid var(--line)}td.t{width:calc(50% - 52px)}\
td.t+td.n{border-left:2px solid var(--line)}\
.del{background:var(--del)}.ins{background:var(--ins)}.chg{background:var(--chg)}.void{background:var(--void)}\
td.del b,td.chg b{background:var(--delb);color:#fff;font-weight:normal;border-radius:2px}td.ins b,.chg+.t b{font-weight:normal}\
tr.fold td{background:var(--bar);color:var(--muted);text-align:center;font:12px 'Segoe UI',sans-serif;padding:3px}\
tr.cur td{outline:1px solid #3883fa;outline-offset:-1px}.same{padding:40px;text-align:center;color:var(--muted);font:15px 'Segoe UI',sans-serif}";

const JS: &str = "\
var hs=[].slice.call(document.querySelectorAll('tr.start')),at=-1;\
function go(d){if(!hs.length)return;if(at>=0)hs[at].classList.remove('cur');at=(at+d+hs.length)%hs.length;hs[at].classList.add('cur');hs[at].scrollIntoView({block:'center'});var c=document.getElementById('pos');if(c)c.textContent=(at+1)+' / '+hs.length}\
document.getElementById('next')&&(document.getElementById('next').onclick=function(){go(1)});\
document.getElementById('prev')&&(document.getElementById('prev').onclick=function(){go(-1)});\
document.addEventListener('keydown',function(e){if(e.key==='Escape'){window.ipc&&window.ipc.postMessage('close');return}if(e.key==='F3'||e.key==='n'||e.key==='j'){go(e.shiftKey?-1:1);e.preventDefault()}else if(e.key==='p'||e.key==='k'){go(-1);e.preventDefault()}});";

/// The page comparing `left` (named `left_name`) with `right`.
pub fn diff_html(left_name: &str, left: &str, right_name: &str, right: &str, dark: bool) -> String {
    // Line endings are not compared (a CRLF file against an LF copy is the same text).
    let (left, right) = (left.replace("\r\n", "\n"), right.replace("\r\n", "\n"));
    let (left, right) = (left.as_str(), right.as_str());
    let too_big = |t: &str| t.bytes().filter(|b| *b == b'\n').count() > MAX_LINES;
    let (body, summary) = if too_big(left) || too_big(right) {
        (format!("<div class=\"same\">These files are too large to compare here (over {MAX_LINES} lines).</div>"), String::new())
    } else {
        let (rows, blocks, removed, added) = rows(left, right);
        let summary = format!("<span class=\"stat\">{blocks} changed block{}</span><span class=\"stat del\">&minus;{removed}</span><span class=\"stat ins\">+{added}</span><span class=\"stat\" id=\"pos\"></span><button id=\"prev\">&lsaquo; Previous</button><button id=\"next\">Next &rsaquo;</button>", if blocks == 1 { "" } else { "s" });
        if blocks == 0 {
            ("<div class=\"same\">The two texts are identical.</div>".to_string(), String::new())
        } else {
            // Fold unchanged runs, keeping CONTEXT lines next to each change.
            let mut keep = vec![false; rows.len()];
            for (i, r) in rows.iter().enumerate() {
                if r.kind != "eq" {
                    for k in i.saturating_sub(CONTEXT)..(i + CONTEXT + 1).min(rows.len()) {
                        keep[k] = true;
                    }
                }
            }
            let mut html = String::from("<table>");
            let mut i = 0;
            while i < rows.len() {
                if !keep[i] {
                    let start = i;
                    while i < rows.len() && !keep[i] {
                        i += 1;
                    }
                    html.push_str(&format!("<tr class=\"fold\"><td colspan=\"4\">&hellip; {} unchanged line{} &hellip;</td></tr>", i - start, if i - start == 1 { "" } else { "s" }));
                    continue;
                }
                let r = &rows[i];
                let starts = r.kind != "eq" && (i == 0 || rows[i - 1].kind == "eq");
                let (lc, rc) = match r.kind {
                    "del" => ("del", "void"),
                    "ins" => ("void", "ins"),
                    "chg" => ("chg", "chg"),
                    _ => ("", ""),
                };
                html.push_str(&format!("<tr{}>{}{}</tr>", if starts { " class=\"start\"" } else { "" }, cell(&r.left, lc), cell(&r.right, rc)));
                i += 1;
            }
            html.push_str("</table>");
            (html, summary)
        }
    };
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'\"><style>{CSS}</style></head><body{}><div id=\"bar\"><b>{}</b><span class=\"stat\">&harr;</span><b>{}</b>{summary}</div>{body}<script>{JS}</script></body></html>",
        if dark { " class=\"dark\"" } else { "" },
        esc(left_name),
        esc(right_name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_say_so() {
        let h = diff_html("a", "same\nlines\n", "b", "same\nlines\n", false);
        assert!(h.contains("identical") && !h.contains("<table>"));
    }

    #[test]
    fn changes_are_counted_marked_and_context_is_folded() {
        let a: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        let b = a.replace("line 15\n", "line FIFTEEN\n").replace("line 28\n", "");
        let h = diff_html("old.txt", &a, "new.txt", &b, true);
        assert!(h.contains("2 changed blocks"), "{h}");
        assert!(h.contains("&minus;2") && h.contains("+1"), "{h}");
        assert!(h.contains("unchanged lines"), "long equal runs are folded");
        assert!(h.contains("<b>"), "the changed characters are marked");
        assert!(h.contains("class=\"dark\""));
        assert!(h.contains("old.txt") && h.contains("new.txt"));
    }

    #[test]
    fn text_is_always_escaped() {
        let h = diff_html("<x>", "<script>alert(1)</script>\n", "\"y\"", "ok & ok\n", false);
        assert!(!h.contains("<script>alert"), "{h}");
        assert!(h.contains("&lt;") && h.contains("&gt;") && h.contains("alert(1)"));
        assert!(h.contains("&lt;x&gt;") && h.contains("&quot;y&quot;"));
    }

    #[test]
    fn handles_missing_trailing_newline_crlf_and_empty_sides() {
        assert!(diff_html("a", "x", "b", "x\n", false).contains("<table>"));
        assert!(diff_html("a", "x\r\ny\r\n", "b", "x\ny\n", false).contains("identical"));
        let h = diff_html("a", "", "b", "new\n", false);
        assert!(h.contains("+1") && h.contains("t ins"));
    }
}
