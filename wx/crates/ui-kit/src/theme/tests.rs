use super::*;
use std::cell::Cell;

fn dark() -> Theme {
    Theme::dark()
}

fn counter() -> (Listener, Rc<Cell<u32>>) {
    let n = Rc::new(Cell::new(0));
    let n2 = n.clone();
    (
        Rc::new(move |_| {
            n2.set(n2.get() + 1);
            true
        }),
        n,
    )
}

#[test]
fn the_first_set_notifies_even_when_it_is_the_default() {
    let mut h = Hub::default();
    let (l, n) = counter();
    assert_eq!(h.subscribe(l).1, None);
    for (_, l) in h.set(Theme::default()) {
        l(&Theme::default());
    }
    assert_eq!(n.get(), 1);
}

#[test]
fn setting_the_same_theme_again_notifies_nobody() {
    let mut h = Hub::default();
    h.set(dark());
    assert!(h.set(dark()).is_empty());
}

#[test]
fn a_change_notifies_every_listener_once() {
    let mut h = Hub::default();
    let (a, na) = counter();
    let (b, nb) = counter();
    h.subscribe(a);
    h.subscribe(b);
    h.set(Theme::default());
    for (_, l) in h.set(dark()) {
        l(&dark());
    }
    assert_eq!((na.get(), nb.get()), (1, 1));
}

#[test]
fn a_late_subscriber_gets_the_current_theme_to_apply() {
    let mut h = Hub::default();
    h.set(dark());
    let (l, _) = counter();
    assert_eq!(h.subscribe(l).1, Some(dark()));
}

#[test]
fn editor_colour_changes_count_as_a_change() {
    let mut h = Hub::default();
    let (l, _) = counter();
    h.subscribe(l);
    h.set(dark());
    let other = Theme { editor_bg: (1, 2, 3), ..dark() };
    assert_eq!(h.set(other).len(), 1);
    assert!(h.set(other).is_empty());
}

#[test]
fn removed_listeners_are_no_longer_called() {
    let mut h = Hub::default();
    let (a, _) = counter();
    let (b, _) = counter();
    let (ida, _) = h.subscribe(a);
    h.subscribe(b);
    h.remove(&[ida]);
    assert_eq!(h.set(dark()).len(), 1);
}

#[test]
fn editor_widgets_take_the_editors_colours() {
    let t = Theme::dark().with_editor((10, 20, 30), (200, 210, 220));
    assert_eq!(t.background(Role::Editor), (10, 20, 30));
    assert_eq!(t.foreground(Role::Editor), (200, 210, 220));
    assert_eq!(t.background(Role::Surface), t.surface());
    assert_eq!(t.foreground(Role::Input), t.text());
}

#[test]
fn every_light_token_has_a_dark_counterpart_that_differs_where_it_should() {
    let (l, d) = (Theme::light().colors, Theme::dark().colors);
    assert_ne!(l.bg, d.bg);
    assert_ne!(l.text, d.text);
    assert_ne!(l.syntax.keyword, d.syntax.keyword);
    assert_eq!(l.accent, d.accent, "the accent is the brand colour in both themes");
}

#[test]
fn text_is_readable_on_every_background_in_both_themes() {
    // WCAG-ish: a large difference in brightness between text and its background.
    let luma = |c: Rgb| (u32::from(c.0) * 299 + u32::from(c.1) * 587 + u32::from(c.2) * 114) / 1000;
    for t in [Theme::light(), Theme::dark()] {
        for bg in [t.colors.bg, t.colors.surface, t.colors.input, t.colors.bg_elevated] {
            assert!(luma(t.colors.text).abs_diff(luma(bg)) > 120, "text on {bg:?} in dark={}", t.dark);
            assert!(luma(t.colors.text_muted).abs_diff(luma(bg)) > 60);
        }
        assert!(luma(t.colors.accent_text).abs_diff(luma(t.colors.accent)) > 90);
    }
}

#[test]
fn spacing_scales_with_the_display() {
    let mut t = Theme::light();
    assert_eq!(t.px(t.spacing.md), 8);
    t.scale = 1.5;
    assert_eq!(t.px(t.spacing.md), 12);
    assert_eq!(t.px(t.spacing.xs), 3);
}

#[test]
fn unpx_undoes_px() {
    let mut t = Theme::light();
    t.scale = 1.5;
    assert_eq!(t.unpx(t.px(800)), 800);
}

#[test]
fn the_hub_gives_every_theme_the_display_scale() {
    let mut hub = Hub { scale: Some(1.5), ..Hub::default() };
    assert!((hub.current().scale - 1.5).abs() < f64::EPSILON);
    let _ = hub.set(dark());
    assert!(hub.current().dark);
    assert!((hub.current().scale - 1.5).abs() < f64::EPSILON);
}

#[test]
fn dark_and_light_have_distinct_surfaces_and_readable_ink() {
    let (d, l) = (dark(), Theme::default());
    assert_ne!(d.surface(), l.surface());
    assert_ne!(d.input(), d.surface());
    assert!(d.text().0 > d.surface().0 && l.text().0 < l.surface().0);
}
