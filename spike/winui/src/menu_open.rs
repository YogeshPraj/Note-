//! Open a WinUI menu from code. windows-reactor 0.100 has no call for it, so the menu bar item is expanded through UI
//! Automation, the way a screen reader opens it: the control is used as Microsoft provides it. UI Automation must not
//! be called on the app's own UI thread (it calls back into that thread), so this runs on a worker thread. On a real
//! screen a menu opens 15 to 35 ms after the key; on a hidden desktop, whose window is never active, it takes 2 s.
#![allow(unsafe_code)]

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationExpandCollapsePattern, TreeScope_Descendants, UIA_ExpandCollapsePatternId,
    UIA_MenuItemControlTypeId,
};

fn expand_now(window: usize, title: &str, steps: &mut Vec<String>) -> windows::core::Result<()> {
    let t = std::time::Instant::now();
    let mut mark = |what: &str| steps.push(format!("{what} {}ms", t.elapsed().as_millis()));
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        mark("created");
        let root = uia.ElementFromHandle(HWND(window as *mut _))?;
        mark("root");
        let all = root.FindAll(TreeScope_Descendants, &uia.CreateTrueCondition()?)?;
        mark(&format!("found {}", all.Length()?));
        let is_it = |e: &IUIAutomationElement| {
            e.CurrentControlType().is_ok_and(|t| t == UIA_MenuItemControlTypeId) && e.CurrentName().is_ok_and(|n| n == title)
        };
        let item = (0..all.Length()?)
            .filter_map(|i| all.GetElement(i).ok())
            .find(is_it)
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_INVALIDARG))?;
        mark("matched");
        let pattern: IUIAutomationExpandCollapsePattern = item.GetCurrentPatternAs(UIA_ExpandCollapsePatternId)?;
        let r = pattern.Expand();
        mark("expanded");
        r
    }
}

/// Expand the menu bar item called `title` in `window` (a window handle as a number); `done` gets the outcome on
/// the worker thread.
pub fn expand(window: usize, title: String, done: impl FnOnce(String) + Send + 'static) {
    std::thread::spawn(move || {
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let started = std::time::Instant::now();
        let mut steps = Vec::new();
        let outcome = match expand_now(window, &title, &mut steps) {
            Ok(()) => format!("expanded {title:?} in {} ms ({})", started.elapsed().as_millis(), steps.join(", ")),
            Err(e) => format!("could not expand {title:?}: {e}"),
        };
        if init.is_ok() {
            unsafe { CoUninitialize() };
        }
        done(outcome);
    });
}
