//! Toolkit-free geometry and colour maths, so layout code can be unit tested without a window.

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect { l, t, r, b }
    }
    pub fn has(&self, x: i32, y: i32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
    pub fn w(&self) -> i32 {
        self.r - self.l
    }
    pub fn h(&self) -> i32 {
        self.b - self.t
    }
}

/// `base` blended towards `ink` by `a` / 255.
pub fn mix(base: Rgb, ink: Rgb, a: u8) -> Rgb {
    let m = |b: u8, i: u8| crate::num::to_u8((u32::from(b) * (255 - u32::from(a)) + u32::from(i) * u32::from(a)) / 255);
    (m(base.0, ink.0), m(base.1, ink.1), m(base.2, ink.2))
}
