use super::*;

#[test]
fn a_selection_too_close_to_the_background_is_pulled_towards_the_text() {
    let (bg, fg) = ((0x14, 0x14, 0x14), (0xF8, 0xF8, 0xF8));
    let shown = visible_selection(bg, fg, (0x2D, 0x30, 0x33));
    assert!(luminance(shown) - luminance(bg) >= 0.14);
}

#[test]
fn a_clear_selection_is_left_alone() {
    let clear = (0x26, 0x4F, 0x78);
    assert_eq!(visible_selection((0x1E, 0x1E, 0x1E), (0xF8, 0xF8, 0xF8), clear), clear);
}
