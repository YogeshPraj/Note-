//! Showing the window's native menus from our own menu bar (Windows).
//!
//! A bare `TrackPopupMenu` has no menu bar around it, so Left/Right would not move to the neighbouring
//! menu. A message filter hook for the menu loop restores that: at the top level of an open menu,
//! Left/Right close it and open the previous/next one (unless Right should open a submenu).

#[cfg(windows)]
mod imp {
    use std::cell::Cell;

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
    }

    #[repr(C)]
    struct MenuItemInfo {
        cb_size: u32,
        mask: u32,
        type_: u32,
        state: u32,
        id: u32,
        sub_menu: isize,
        bmp_checked: isize,
        bmp_unchecked: isize,
        item_data: usize,
        type_data: *mut u16,
        cch: u32,
        bmp_item: isize,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetMenu(hwnd: isize) -> isize;
        fn SetMenu(hwnd: isize, menu: isize) -> i32;
        fn GetSubMenu(menu: isize, pos: i32) -> isize;
        fn GetMenuItemCount(menu: isize) -> i32;
        fn GetMenuItemInfoW(menu: isize, item: u32, by_pos: i32, info: *mut MenuItemInfo) -> i32;
        fn TrackPopupMenu(menu: isize, flags: u32, x: i32, y: i32, reserved: i32, hwnd: isize, rect: *const u8) -> i32;
        fn EndMenu() -> i32;
        fn SetWindowsHookExW(id: i32, proc: unsafe extern "system" fn(i32, usize, isize) -> isize, module: isize, thread: u32) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
        fn FindWindowExW(parent: isize, after: isize, class: *const u16, title: *const u16) -> isize;
        fn IsWindowVisible(hwnd: isize) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    thread_local! {
        /// The menu being shown (a submenu of the bar) and where the hook wants to go next.
        static SHOWN: Cell<isize> = const { Cell::new(0) };
        static GOTO: Cell<i32> = const { Cell::new(-1) };
        static COUNT: Cell<i32> = const { Cell::new(0) };
        static CURRENT: Cell<i32> = const { Cell::new(0) };
        static HOOK: Cell<isize> = const { Cell::new(0) };
    }

    const WH_MSGFILTER: i32 = -1;
    const MSGF_MENU: i32 = 2;
    const WM_KEYDOWN: u32 = 0x100;
    const VK_LEFT: usize = 0x25;
    const VK_RIGHT: usize = 0x27;
    const MIIM_STATE: u32 = 0x1;
    const MIIM_SUBMENU: u32 = 0x4;
    const MFS_HILITE: u32 = 0x80;

    pub fn detach(hwnd: isize) -> isize {
        unsafe {
            let m = GetMenu(hwnd);
            SetMenu(hwnd, 0);
            m
        }
    }

    /// Open popups of the menu loop (the menu and any submenus).
    fn open_popups() -> usize {
        let class: Vec<u16> = "#32768".encode_utf16().chain(Some(0)).collect();
        let mut n = 0;
        let mut after = 0;
        unsafe {
            loop {
                let w = FindWindowExW(0, after, class.as_ptr(), std::ptr::null());
                if w == 0 {
                    break;
                }
                if IsWindowVisible(w) != 0 {
                    n += 1;
                }
                after = w;
            }
        }
        n
    }

    /// Does the highlighted item of `menu` open a submenu?
    fn highlighted_has_submenu(menu: isize) -> bool {
        unsafe {
            for i in 0..GetMenuItemCount(menu).max(0) {
                let mut info = MenuItemInfo {
                    cb_size: size_of::<MenuItemInfo>() as u32,
                    mask: MIIM_STATE | MIIM_SUBMENU,
                    type_: 0,
                    state: 0,
                    id: 0,
                    sub_menu: 0,
                    bmp_checked: 0,
                    bmp_unchecked: 0,
                    item_data: 0,
                    type_data: std::ptr::null_mut(),
                    cch: 0,
                    bmp_item: 0,
                };
                if GetMenuItemInfoW(menu, i as u32, 1, &raw mut info) != 0 && info.state & MFS_HILITE != 0 {
                    return info.sub_menu != 0;
                }
            }
        }
        false
    }

    unsafe extern "system" fn hook(code: i32, wparam: usize, lparam: isize) -> isize {
        if code == MSGF_MENU {
            let m = unsafe { &*(lparam as *const Msg) };
            if m.message == WM_KEYDOWN && (m.wparam == VK_LEFT || m.wparam == VK_RIGHT) {
                let right = m.wparam == VK_RIGHT;
                // Only at the top level, and not when Right would open a submenu.
                if open_popups() == 1 && !(right && highlighted_has_submenu(SHOWN.with(std::cell::Cell::get))) {
                    let n = COUNT.with(std::cell::Cell::get).max(1);
                    let cur = CURRENT.with(std::cell::Cell::get);
                    GOTO.with(|g| g.set((cur + if right { 1 } else { n - 1 }) % n));
                    unsafe { EndMenu() };
                    return 1;
                }
            }
        }
        unsafe { CallNextHookEx(HOOK.with(std::cell::Cell::get), code, wparam, lparam) }
    }

    /// Show menu `index` of the detached bar; `at(i)` gives the screen position of menu `i`.
    pub fn popup(bar: isize, hwnd: isize, index: i32, count: i32, at: &dyn Fn(i32) -> (i32, i32)) {
        unsafe {
            let h = SetWindowsHookExW(WH_MSGFILTER, hook, 0, GetCurrentThreadId());
            HOOK.with(|c| c.set(h));
            COUNT.with(|c| c.set(count));
            let mut index = index;
            loop {
                let sub = GetSubMenu(bar, index);
                if sub == 0 {
                    break;
                }
                SHOWN.with(|c| c.set(sub));
                CURRENT.with(|c| c.set(index));
                GOTO.with(|g| g.set(-1));
                let (x, y) = at(index);
                TrackPopupMenu(sub, 0, x, y, 0, hwnd, std::ptr::null());
                let next = GOTO.with(std::cell::Cell::get);
                if next < 0 {
                    break;
                }
                index = next;
            }
            if h != 0 {
                UnhookWindowsHookEx(h);
            }
            HOOK.with(|c| c.set(0));
        }
    }
}

pub use imp::{detach, popup};
