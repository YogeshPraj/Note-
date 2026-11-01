//! Showing the window's native menus from our own menu bar (Windows).

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetMenu(hwnd: isize) -> isize;
    fn SetMenu(hwnd: isize, menu: isize) -> i32;
    fn GetSubMenu(menu: isize, pos: i32) -> isize;
    fn TrackPopupMenu(menu: isize, flags: u32, x: i32, y: i32, reserved: i32, hwnd: isize, rect: *const u8) -> i32;
}

/// Take the native menu bar off the window (its menus keep working) and return its handle.
#[cfg(windows)]
pub fn detach(hwnd: isize) -> isize {
    unsafe {
        let m = GetMenu(hwnd);
        SetMenu(hwnd, 0);
        m
    }
}

/// Pop up menu `index` of the detached bar at screen position (x, y).
#[cfg(windows)]
pub fn popup(bar: isize, hwnd: isize, index: i32, x: i32, y: i32) {
    unsafe {
        let sub = GetSubMenu(bar, index);
        if sub != 0 {
            TrackPopupMenu(sub, 0, x, y, 0, hwnd, std::ptr::null());
        }
    }
}

#[cfg(not(windows))]
pub fn detach(_hwnd: isize) -> isize {
    0
}

#[cfg(not(windows))]
pub fn popup(_bar: isize, _hwnd: isize, _index: i32, _x: i32, _y: i32) {}
