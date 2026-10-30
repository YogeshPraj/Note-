//! Accept files dragged from Explorer onto the editor.
//!
//! Scintilla registers its own OLE drop target on its window, which only takes text, so a file
//! dragged over the editor was refused. Ours sits in front of it: a dragged file list is queued
//! and the host is told to open it; anything else is passed to Scintilla's target untouched, so
//! dragging text around inside the editor keeps working.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, POINTL, S_OK, WPARAM};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{GetPropW, PostMessageW, WM_APP};
use windows::core::{Interface, Ref, Result, implement, w};

/// Posted to the host when dropped files are waiting in `take()`.
pub const WM_FILES_DROPPED: u32 = WM_APP + 90;

static DROPPED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The files dropped since the last call.
pub fn take() -> Vec<PathBuf> {
    DROPPED.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

fn hdrop_format() -> FORMATETC {
    FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

fn has_files(data: &IDataObject) -> bool {
    unsafe { data.QueryGetData(&hdrop_format()) == S_OK }
}

fn read_files(data: &IDataObject) -> Vec<PathBuf> {
    let mut out = Vec::new();
    unsafe {
        let Ok(mut medium) = data.GetData(&hdrop_format()) else { return out };
        let drop = HDROP(medium.u.hGlobal.0);
        let n = DragQueryFileW(drop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
            out.push(PathBuf::from(String::from_utf16_lossy(&buf[..got])));
        }
        ReleaseStgMedium(&mut medium);
    }
    out
}

#[implement(IDropTarget)]
struct FileDrop {
    /// Scintilla's own target, for everything that is not a file list.
    inner: Option<IDropTarget>,
    host: HWND,
    over_files: Cell<bool>,
}

impl IDropTarget_Impl for FileDrop_Impl {
    fn DragEnter(&self, data: Ref<'_, IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let files = data.as_ref().is_some_and(has_files);
        self.over_files.set(files);
        if files {
            unsafe { *effect = DROPEFFECT_COPY };
            return Ok(());
        }
        match (&self.inner, data.as_ref()) {
            (Some(inner), Some(d)) => unsafe { inner.DragEnter(d, keys, *pt, effect) },
            _ => {
                unsafe { *effect = DROPEFFECT_NONE };
                Ok(())
            }
        }
    }

    fn DragOver(&self, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        if self.over_files.get() {
            unsafe { *effect = DROPEFFECT_COPY };
            return Ok(());
        }
        match &self.inner {
            Some(inner) => unsafe { inner.DragOver(keys, *pt, effect) },
            None => {
                unsafe { *effect = DROPEFFECT_NONE };
                Ok(())
            }
        }
    }

    fn DragLeave(&self) -> Result<()> {
        if self.over_files.replace(false) {
            return Ok(());
        }
        match &self.inner {
            Some(inner) => unsafe { inner.DragLeave() },
            None => Ok(()),
        }
    }

    fn Drop(&self, data: Ref<'_, IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        if self.over_files.replace(false) {
            if let Some(d) = data.as_ref() {
                let files = read_files(d);
                if let Ok(mut q) = DROPPED.lock() {
                    q.extend(files);
                }
                unsafe {
                    let _ = PostMessageW(Some(self.host), WM_FILES_DROPPED, WPARAM(0), LPARAM(0));
                    *effect = DROPEFFECT_COPY;
                }
            }
            return Ok(());
        }
        match (&self.inner, data.as_ref()) {
            (Some(inner), Some(d)) => unsafe { inner.Drop(d, keys, *pt, effect) },
            _ => Ok(()),
        }
    }
}

/// Put the file-accepting target in front of `edit`'s own one. Files then open in `host`.
pub fn install(edit: HWND, host: HWND) {
    unsafe {
        let _ = OleInitialize(None);
        // OLE keeps the registered target in this window property.
        let prop = GetPropW(edit, w!("OleDropTargetInterface"));
        let inner = if prop.0.is_null() { None } else { IDropTarget::from_raw_borrowed(&prop.0).cloned() };
        let _ = RevokeDragDrop(edit);
        let target: IDropTarget = FileDrop { inner, host, over_files: Cell::new(false) }.into();
        let _ = RegisterDragDrop(edit, &target);
    }
}
