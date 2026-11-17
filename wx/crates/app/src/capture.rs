//! Capture Screen and Capture Region: Note++ steps aside, the screen is copied, and the picture opens in a new tab of
//! its own (the picture editor, as for a pasted picture). A region is chosen by dragging over a still copy of the screen.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use notepp_core::commands::ID_CAPTURE_REGION;
use wxdragon::dc::{AutoBufferedPaintDC, BlitConfig, DeviceContext, MemoryDC, ScreenDC};
use wxdragon::prelude::*;

use crate::app::Shared;

/// How long the window gets to leave the screen before it is copied.
const LEAVE: Duration = Duration::from_millis(350);
const KEY_ESCAPE: i32 = 27;
/// The smallest region worth keeping, in pixels.
const LEAST: i32 = 3;

/// A copy of part of the screen: its top left corner on the screen, its size and RGBA pixels.
struct Shot {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

/// The display the window is on, as its corner and size.
fn display_box(frame: &Frame) -> Option<(i32, i32, i32, i32)> {
    let d = Display::from_window(frame).or_else(|| Display::new(0))?;
    let g = d.geometry();
    (g.width > 0 && g.height > 0).then_some((g.x, g.y, g.width, g.height))
}

/// Copy the display the window is on.
fn grab(frame: &Frame) -> Option<Shot> {
    let (x, y, w, h) = display_box(frame)?;
    let mut bitmap = Bitmap::new(w, h)?;
    {
        let mut memory = MemoryDC::new();
        memory.select_object(&mut bitmap);
        if !memory.blit(&ScreenDC::new(), BlitConfig::new(0, 0, w, h, x, y)) {
            return None;
        }
    }
    let mut rgba = bitmap.get_rgba_data()?;
    // A screen has no transparency, whatever the copy says.
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Some(Shot { x, y, w: u32::try_from(w).ok()?, h: u32::try_from(h).ok()?, rgba })
}

/// The part of `shot` from (x, y), `w` by `h` pixels.
fn crop(shot: &Shot, x: u32, y: u32, w: u32, h: u32) -> Option<Shot> {
    let (x, y) = (x.min(shot.w), y.min(shot.h));
    let (w, h) = (w.min(shot.w - x), h.min(shot.h - y));
    if w == 0 || h == 0 {
        return None;
    }
    let to = |v: u32| usize::try_from(v).ok();
    let (stride, row_len) = (to(shot.w)? * 4, to(w)? * 4);
    let mut rgba = Vec::with_capacity(row_len * to(h)?);
    for row in y..y + h {
        let from = to(row)? * stride + to(x)? * 4;
        rgba.extend_from_slice(shot.rgba.get(from..from + row_len)?);
    }
    Some(Shot { x: shot.x + i32::try_from(x).ok()?, y: shot.y + i32::try_from(y).ok()?, w, h, rgba })
}

/// The same picture, darker: what lies outside the chosen region.
fn dimmed(shot: &Shot) -> Option<Bitmap> {
    let dim = |v: u8| u8::try_from(u16::from(v) * 2 / 5).unwrap_or(0);
    let dark: Vec<u8> = shot.rgba.as_chunks::<4>().0.iter().flat_map(|p| [dim(p[0]), dim(p[1]), dim(p[2]), 255]).collect();
    Bitmap::from_rgba(&dark, shot.w, shot.h)
}

/// A length in window pixels as one in picture pixels.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled(v: i32, by: f64) -> u32 {
    (f64::from(v.max(0)) * by).round() as u32
}

/// A capture is under way (a second key press must not start another).
static BUSY: AtomicBool = AtomicBool::new(false);

/// Bring the main window back.
fn restore(frame: &Frame) {
    BUSY.store(false, Ordering::SeqCst);
    frame.show(true);
    if frame.is_iconized() {
        frame.iconize(false);
    }
    frame.raise();
}

/// Open `shot` as a picture tab, or say that nothing could be copied.
fn deliver(a: &Shared, shot: Option<Shot>) {
    let Ok(mut app) = a.try_borrow_mut() else { return };
    if let Some(s) = shot {
        app.open_picture(s.w, s.h, &s.rgba);
    } else {
        app.flash = Some("Could not copy the screen".into());
        app.refresh();
    }
}

/// Capture Screen or Capture Region, run with nothing borrowed.
pub(crate) fn run(a: &Shared, id: usize) {
    let Ok(frame) = a.try_borrow().map(|app| app.frame) else { return };
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    frame.show(false);
    std::thread::sleep(LEAVE);
    match grab(&frame) {
        Some(shot) if id == ID_CAPTURE_REGION => choose(a, frame, shot),
        shot => {
            restore(&frame);
            deliver(a, shot);
        }
    }
}

#[derive(Default)]
struct Drag {
    start: Option<(i32, i32)>,
    now: (i32, i32),
}

impl Drag {
    /// The rectangle dragged so far (x, y, width, height), in the window's pixels.
    fn rect(&self) -> Option<(i32, i32, i32, i32)> {
        let (sx, sy) = self.start?;
        Some((sx.min(self.now.0), sy.min(self.now.1), (sx - self.now.0).abs(), (sy - self.now.1).abs()))
    }
}

fn pos(e: &WindowEventData) -> (i32, i32) {
    let p = match e {
        WindowEventData::MouseButton(m) => m.get_position(),
        WindowEventData::MouseMotion(m) => m.get_position(),
        _ => None,
    };
    p.map_or((0, 0), |p| (p.x, p.y))
}

/// Cover the display with its own picture, dimmed, and let the user drag over the part to keep. Esc or the right button
/// gives up.
#[allow(clippy::too_many_lines, reason = "builds the overlay and wires its mouse and key events in one place")]
fn choose(a: &Shared, frame: Frame, shot: Shot) {
    let (Some(dark), Some(bright)) = (dimmed(&shot), Bitmap::from_rgba(&shot.rgba, shot.w, shot.h)) else {
        restore(&frame);
        return deliver(a, None);
    };
    let (w, h) = (i32::try_from(shot.w).unwrap_or(1), i32::try_from(shot.h).unwrap_or(1));
    let overlay =
        Frame::builder().with_style(FrameStyle::StayOnTop | FrameStyle::NoTaskbar).with_position(Point::new(shot.x, shot.y)).with_size(Size::new(w, h)).build();
    let panel = Panel::builder(&overlay).build();
    panel.set_background_style(BackgroundStyle::Paint);
    if let Some(c) = Cursor::from_stock(StockCursor::Cross) {
        panel.set_cursor(Some(&c));
    }
    let (a, shot) = (a.clone(), Rc::new(shot));
    let drag = Rc::new(RefCell::new(Drag::default()));

    // The one way out: the window comes back, and the picture goes on when a region was chosen.
    let finish = {
        let shot = shot.clone();
        Rc::new(move |region: Option<(i32, i32, i32, i32)>| {
            let area = panel.get_client_size();
            overlay.show(false);
            overlay.destroy();
            restore(&frame);
            let (kx, ky) = (f64::from(shot.w) / f64::from(area.width.max(1)), f64::from(shot.h) / f64::from(area.height.max(1)));
            let part = region.and_then(|(x, y, rw, rh)| crop(&shot, scaled(x, kx), scaled(y, ky), scaled(rw, kx), scaled(rh, ky)));
            if part.is_some() {
                deliver(&a, part);
            }
        })
    };

    let d = drag.clone();
    panel.on_paint(move |_| {
        let dc = AutoBufferedPaintDC::new(&panel);
        dc.draw_bitmap(&dark, 0, 0, false);
        if let Some((x, y, rw, rh)) = d.borrow().rect().filter(|r| r.2 > 0 && r.3 > 0) {
            let mut memory = MemoryDC::new();
            memory.select_object_as_source(&bright);
            dc.blit(&memory, BlitConfig::new(x, y, rw, rh, x, y));
            dc.set_pen(Colour::new(255, 255, 255, 255), 1, PenStyle::Solid);
            dc.set_brush(Colour::new(0, 0, 0, 0), BrushStyle::Transparent);
            dc.draw_rectangle(x, y, rw, rh);
        }
        let hint = "Drag over the part of the screen to keep. Esc cancels.";
        let (tw, th) = dc.get_text_extent(hint);
        dc.set_pen(Colour::new(0, 0, 0, 255), 1, PenStyle::Solid);
        dc.set_brush(Colour::new(0, 0, 0, 255), BrushStyle::Solid);
        dc.draw_rectangle(12, 12, tw + 16, th + 10);
        dc.set_text_foreground(Colour::new(255, 255, 255, 255));
        dc.draw_text(hint, 20, 17);
    });

    let d = drag.clone();
    panel.on_mouse_left_down(move |e| {
        let p = pos(&e);
        *d.borrow_mut() = Drag { start: Some(p), now: p };
        panel.capture_mouse();
        panel.refresh(false, None);
    });
    let d = drag.clone();
    panel.on_mouse_motion(move |e| {
        if d.borrow().start.is_some() {
            d.borrow_mut().now = pos(&e);
            panel.refresh(false, None);
        }
    });
    let (d, f) = (drag, finish.clone());
    panel.on_mouse_left_up(move |e| {
        if d.borrow().start.is_none() {
            return;
        }
        panel.release_mouse();
        d.borrow_mut().now = pos(&e);
        let r = d.borrow().rect().filter(|r| r.2 >= LEAST && r.3 >= LEAST);
        f(r);
    });
    let f = finish.clone();
    panel.on_mouse_right_down(move |_| f(None));
    let f = finish;
    overlay.on_char_hook(move |e| match &e {
        WindowEventData::Keyboard(k) if k.get_key_code() == Some(KEY_ESCAPE) => f(None),
        _ => e.skip(true),
    });

    overlay.show(true);
    overlay.set_size_with_pos(shot.x, shot.y, w, h);
    panel.set_size_with_pos(0, 0, w, h);
    panel.set_focus();
}
