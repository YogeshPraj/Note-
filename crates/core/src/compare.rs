//! Compare: a side-by-side text diff as a self-contained HTML page.
//!
//! The page has no external resources and no script that reads the compared text; the text is
//! only ever placed in the page as escaped HTML. Long runs of unchanged lines are folded.

use similar::{DiffOp, TextDiff};

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

/// Lines longer than this are not compared inside: it is slow, and the marks would be unreadable.
const MAX_INLINE: usize = 2000;
/// Two lines sharing less than this share of their text are different lines, not one line that was edited.
const RELATED_PERCENT: usize = 35;
const RELATED: f32 = 0.35;
/// A changed block with more line pairs than this is paired in order, not by likeness (the pairing looks at every pair).
const MAX_PAIRINGS: usize = 4000;

/// Mark what differs between two versions of a line: (left html, right html). The comparison drops coincidental
/// matches (two different sentences share many letters), so whole words and phrases are marked, not single letters.
/// None when the lines have too little in common to be versions of one line, or are too long.
fn inline(a: &str, b: &str) -> Option<(String, String)> {
    if a.len() > MAX_INLINE || b.len() > MAX_INLINE {
        return None;
    }
    let chunks = dissimilar::diff(a, b);
    let same: usize = chunks.iter().map(|c| if let dissimilar::Chunk::Equal(t) = c { t.trim().chars().count() } else { 0 }).sum();
    // What the two share, against both: a line that grew a long tail is still the same line.
    let both = (a.trim().chars().count() + b.trim().chars().count()).max(1);
    if same * 2 * 100 < both * RELATED_PERCENT {
        return None;
    }
    let (mut l, mut r) = (String::new(), String::new());
    for c in chunks {
        match c {
            dissimilar::Chunk::Equal(t) => {
                l.push_str(&esc(t));
                r.push_str(&esc(t));
            }
            dissimilar::Chunk::Delete(t) => l.push_str(&format!("<b>{}</b>", esc(t))),
            dissimilar::Chunk::Insert(t) => r.push_str(&format!("<b>{}</b>", esc(t))),
        }
    }
    Some((l, r))
}

/// Which old line of a changed block is which new line: pairs (old, new), in order, of lines alike enough to be one
/// line edited. The pairing with the most likeness in total wins, so a line added in the middle of a block does not
/// shift every line after it onto the wrong partner.
fn pair_lines(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let (m, n) = (old.len(), new.len());
    if m == 0 || n == 0 || m * n > MAX_PAIRINGS {
        return Vec::new();
    }
    let alike = |i: usize, j: usize| {
        let (a, b) = (old[i].trim(), new[j].trim());
        if a.is_empty() || b.is_empty() || a.len() > MAX_INLINE || b.len() > MAX_INLINE {
            return 0.0;
        }
        // On the words alone: the spaces between them match in any two sentences.
        let (wa, wb): (Vec<&str>, Vec<&str>) = (a.split_whitespace().collect(), b.split_whitespace().collect());
        let ratio = TextDiff::from_slices(&wa, &wb).ratio();
        if ratio < RELATED { 0.0 } else { ratio }
    };
    let sim: Vec<Vec<f32>> = (0..m).map(|i| (0..n).map(|j| alike(i, j)).collect()).collect();
    // best[i][j]: the most likeness reachable pairing old[i..] with new[j..].
    let mut best = vec![vec![0.0f32; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            let paired = if sim[i][j] > 0.0 { sim[i][j] + best[i + 1][j + 1] } else { 0.0 };
            best[i][j] = paired.max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < m && j < n {
        if sim[i][j] > 0.0 && sim[i][j] + best[i + 1][j + 1] >= best[i + 1][j].max(best[i][j + 1]) {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if best[i + 1][j] >= best[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

struct Row {
    left: Option<(usize, String)>,
    right: Option<(usize, String)>,
    kind: &'static str, // "eq" | "del" | "ins" | "chg"
}

/// A run of lines inside a changed block: the block's first line number less one, then the run's start and end.
type Run = (usize, usize, usize);

/// The rows of a run of old lines against a run of new lines, none of which are paired by likeness: they sit side by
/// side as far as both last (a line replaced by a different one), the rest alone.
fn unpaired(old: &[&str], (o0, from, to): Run, new: &[&str], (n0, nfrom, nto): Run, out: &mut Vec<Row>) {
    for k in 0..(to - from).max(nto - nfrom) {
        let l = (from + k < to).then(|| (o0 + from + k + 1, esc(strip_eol(old[from + k]))));
        let r = (nfrom + k < nto).then(|| (n0 + nfrom + k + 1, esc(strip_eol(new[nfrom + k]))));
        let kind = match (&l, &r) {
            (Some(_), Some(_)) => "chg",
            (Some(_), None) => "del",
            _ => "ins",
        };
        out.push(Row { left: l, right: r, kind });
    }
}

/// The rows of a changed block: `old` (its first line is line `o0 + 1`) became `new`.
fn replaced(old: &[&str], o0: usize, new: &[&str], n0: usize, out: &mut Vec<Row>) {
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pair_lines(old, new) {
        unpaired(old, (o0, i, pi), new, (n0, j, pj), out);
        let (l, r) = (strip_eol(old[pi]), strip_eol(new[pj]));
        let (lh, rh) = inline(l, r).unwrap_or_else(|| (esc(l), esc(r)));
        out.push(Row { left: Some((o0 + pi + 1, lh)), right: Some((n0 + pj + 1, rh)), kind: "chg" });
        (i, j) = (pi + 1, pj + 1);
    }
    unpaired(old, (o0, i, old.len()), new, (n0, j, new.len()), out);
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
                replaced(&la[old_index..old_index + old_len], old_index, &lb[new_index..new_index + new_len], new_index, &mut out);
            }
        }
    }
    (out, blocks, removed, added)
}

fn cell(side: Option<&(usize, String)>, class: &str) -> String {
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
td.del b,td.chg b{background:var(--delb);color:#fff;font-weight:normal;border-radius:2px}td.t.chg~td.t.chg b{background:var(--insb)}td.ins b{font-weight:normal}\
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
        let summary = format!(
            "<span class=\"stat\">{blocks} changed block{}</span><span class=\"stat del\">&minus;{removed}</span><span class=\"stat ins\">+{added}</span><span class=\"stat\" id=\"pos\"></span><button id=\"prev\">&lsaquo; Previous</button><button id=\"next\">Next &rsaquo;</button>",
            if blocks == 1 { "" } else { "s" }
        );
        if blocks == 0 {
            ("<div class=\"same\">The two texts are identical.</div>".to_string(), String::new())
        } else {
            // Fold unchanged runs, keeping CONTEXT lines next to each change.
            let mut keep = vec![false; rows.len()];
            for (i, r) in rows.iter().enumerate() {
                if r.kind != "eq" {
                    keep[i.saturating_sub(CONTEXT)..(i + CONTEXT + 1).min(rows.len())].fill(true);
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
                    html.push_str(&format!(
                        "<tr class=\"fold\"><td colspan=\"4\">&hellip; {} unchanged line{} &hellip;</td></tr>",
                        i - start,
                        if i - start == 1 { "" } else { "s" }
                    ));
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
                html.push_str(&format!("<tr{}>{}{}</tr>", if starts { " class=\"start\"" } else { "" }, cell(r.left.as_ref(), lc), cell(r.right.as_ref(), rc)));
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
        let a: String = (1..=30).map(|i| format!("line {i}\n")).collect::<Vec<_>>().concat();
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
    #[test]
    fn a_reworded_line_marks_phrases_not_scattered_letters() {
        let a = "Route only when the customer has named a product AND that product's issue clearly matches a listed specialist.\n";
        let b = "Route only when the customer has named a product and a listed specialist covers it.\n";
        let h = diff_html("a", a, "b", b, false);
        let marks = h.matches("<b>").count() - 2;
        assert!((1..=8).contains(&marks), "{marks} marks: {h}");
        assert!(h.contains("Route only when the customer has named a product "), "the shared start is left unmarked: {h}");
    }

    #[test]
    fn unrelated_lines_are_not_marked_inside() {
        let h = diff_html("a", "The quick brown fox jumps over the lazy dog.\n", "b", "fn main() { println!(42); }\n", false);
        assert_eq!(h.matches("<b>").count(), 2, "only the two names in the bar are bold: {h}");
        assert!(h.contains("t chg"), "they still sit side by side as a changed line");
    }

    #[test]
    fn a_line_added_inside_a_block_does_not_shift_the_pairs() {
        let a = "alpha one two three\nbeta four five six\ngamma seven eight nine\n";
        let b = "alpha one two 3\nsomething else entirely here\nbeta four five 6\ngamma seven eight 9\n";
        let (rows, _, _, _) = rows(a, b);
        let pairs: Vec<(Option<usize>, Option<usize>)> = rows.iter().map(|r| (r.left.as_ref().map(|l| l.0), r.right.as_ref().map(|r| r.0))).collect();
        assert_eq!(pairs, vec![(Some(1), Some(1)), (None, Some(2)), (Some(2), Some(3)), (Some(3), Some(4))]);
    }

    #[test]
    fn pairing_keeps_order_and_skips_what_has_no_partner() {
        assert_eq!(pair_lines(&["same words here", "zzz"], &["qqq", "same words here!"]), vec![(0, 1)]);
        assert!(pair_lines(&[], &["x"]).is_empty());
    }
}
