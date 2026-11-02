//! A slim bar beside the caret's line number, so the current row is easy to find in the gutter.

use super::sci::{self, Send, Theme, rgb};

/// Marker 24 sits below the folding markers (25 to 31) and above the bookmark and hidden-line ones.
const MARKER: usize = 24;
const MASK: usize = 1 << MARKER;
const SC_MARK_LEFTRECT: usize = 27;

/// How far the bar's colour goes from the gutter colour towards the text colour, in percent.
const TINT_PERCENT: i32 = 55;

/// The colour of the current line's bar for `theme`.
pub fn tint(theme: &Theme) -> (u8, u8, u8) {
    let mix = |g: u8, f: u8| crate::num::to_u8(i32::from(g) + (i32::from(f) - i32::from(g)) * TINT_PERCENT / 100);
    let (g, f) = (theme.line_no_bg, theme.fg);
    (mix(g.0, f.0), mix(g.1, f.1), mix(g.2, f.2))
}

/// Let the line-number margin paint the marker. Call after `sci::setup_line_numbers`.
pub fn setup<S: Send>(s: &S, theme: &Theme) {
    let c = tint(theme);
    s.send(sci::SCI_SETMARGINMASKN, 1, crate::num::to_isize(MASK));
    s.send(sci::SCI_MARKERDEFINE, MARKER, crate::num::to_isize(SC_MARK_LEFTRECT));
    s.send(sci::SCI_MARKERSETFORE, MARKER, rgb(c.0, c.1, c.2));
    s.send(sci::SCI_MARKERSETBACK, MARKER, rgb(c.0, c.1, c.2));
}

/// Put the marker on the caret's line (and nowhere else). Does nothing when it is already there.
pub fn follow<S: Send>(s: &S) {
    let line = crate::bookmarks::current_line(s);
    let mask = crate::num::to_isize(MASK);
    let first = s.send(sci::SCI_MARKERNEXT, 0, mask);
    if first == crate::num::to_isize(line) && s.send(sci::SCI_MARKERNEXT, line + 1, mask) < 0 {
        return;
    }
    s.send(sci::SCI_MARKERDELETEALL, MARKER, 0);
    s.send(sci::SCI_MARKERADD, line, crate::num::to_isize(MARKER));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tint_lies_between_the_gutter_and_the_text_colour() {
        let theme = Theme::dark();
        let t = tint(&theme);
        assert!(t.0 > theme.line_no_bg.0 && t.0 < theme.fg.0);
    }
}
