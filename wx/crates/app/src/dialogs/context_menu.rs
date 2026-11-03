//! Settings > Edit Popup Context Menu: choosing and ordering the editor's right-click menu.

use super::*;

// ── Edit Popup ContextMenu ───────────────────────────────────────────────────

#[derive(Default)]
pub(super) struct CtxState {
    all: Vec<String>,
    menu: Vec<String>,
    default: Vec<String>,
    /// Indices into `all` of the rows the left list shows.
    shown: Vec<usize>,
}

thread_local! {
    static CTX: RefCell<CtxState> = RefCell::new(CtxState::default());
    static CTX_APPLY: RefCell<Option<MenuApply>> = const { RefCell::new(None) };
    static CTX_TEXT: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Tell the owner the menu as it is now.
pub(super) fn commit_menu() {
    let menu = CTX.with(|c| c.borrow().menu.clone());
    if let Some(apply) = CTX_APPLY.with(|a| a.borrow().clone()) {
        apply(menu);
    }
}

pub(super) const CTX_SEP: &str = "-";
pub(super) const CTX_AVAIL: usize = 100;
pub(super) const CTX_MENU: usize = 101;
pub(super) const CTX_FILTER: usize = 102;

pub(super) fn ctx_fill_avail(f: &Form) {
    let q = f.text(CTX_FILTER).trim().to_lowercase();
    let rows = CTX.with(|c| {
        let mut c = c.borrow_mut();
        let shown: Vec<usize> = (0..c.all.len()).filter(|&i| !c.menu.contains(&c.all[i]) && (q.is_empty() || c.all[i].to_lowercase().contains(&q))).collect();
        let rows = shown.iter().map(|&i| vec![c.all[i].clone()]).collect::<Vec<_>>();
        c.shown = shown;
        rows
    });
    f.list_fill(CTX_AVAIL, &[("Available commands", 300)], &rows);
}

pub(super) fn ctx_fill_menu(f: &Form, select: Option<usize>) {
    let rows = CTX.with(|c| {
        c.borrow()
            .menu
            .iter()
            .map(|n| {
                vec![if n == CTX_SEP {
                    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}  separator  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()
                } else {
                    n.clone()
                }]
            })
            .collect::<Vec<_>>()
    });
    f.list_fill(CTX_MENU, &[("In the right-click menu", 300)], &rows);
    if let Some(i) = select.filter(|i| *i < rows.len()) {
        f.list_select(CTX_MENU, i);
    }
}

/// Add the picked commands after the selected menu row.
pub(super) fn ctx_add(f: &Form, sel: Option<usize>) {
    let picked = f.list_selected_all(CTX_AVAIL);
    if picked.is_empty() {
        return;
    }
    let at = sel.map_or(usize::MAX, |s| s + 1);
    let first = CTX.with(|c| {
        let mut c = c.borrow_mut();
        let names: Vec<String> = picked.iter().filter_map(|&r| c.shown.get(r).map(|&i| c.all[i].clone())).collect();
        let at = at.min(c.menu.len());
        for (k, n) in names.iter().enumerate() {
            c.menu.insert(at + k, n.clone());
        }
        at
    });
    ctx_fill_avail(f);
    ctx_fill_menu(f, Some(first));
}

pub(super) fn ctx_separator(f: &Form, sel: Option<usize>) {
    let at = CTX.with(|c| {
        let mut c = c.borrow_mut();
        let at = sel.map_or(c.menu.len(), |s| s + 1).min(c.menu.len());
        c.menu.insert(at, CTX_SEP.into());
        at
    });
    ctx_fill_menu(f, Some(at));
}

pub(super) fn ctx_remove(f: &Form, sel: Option<usize>) {
    let Some(s) = sel else { return };
    CTX.with(|c| {
        c.borrow_mut().menu.remove(s);
    });
    ctx_fill_avail(f);
    ctx_fill_menu(f, Some(s.min(CTX.with(|c| c.borrow().menu.len()).saturating_sub(1))));
}

pub(super) fn ctx_move(f: &Form, sel: Option<usize>, up: bool) {
    let Some(s) = sel else { return };
    let to = CTX.with(|c| {
        let mut c = c.borrow_mut();
        let to = if up { s.checked_sub(1) } else { (s + 1 < c.menu.len()).then_some(s + 1) };
        if let Some(t) = to {
            c.menu.swap(s, t);
        }
        to
    });
    if let Some(t) = to {
        ctx_fill_menu(f, Some(t));
    }
}

pub(super) fn ctx_reset(f: &Form) {
    CTX.with(|c| {
        let mut c = c.borrow_mut();
        let c = &mut *c;
        c.menu.clone_from(&c.default);
    });
    ctx_fill_avail(f);
    ctx_fill_menu(f, Some(0));
}

pub(super) fn ctx_click(f: &Form, id: usize) {
    const ADD: usize = 103;
    const REMOVE: usize = 104;
    const UP: usize = 105;
    const DOWN: usize = 106;
    const SEP: usize = 107;
    const RESET: usize = 108;
    const TEXT: usize = 109;
    let sel = f.list_selected(CTX_MENU);
    match id {
        ADD => ctx_add(f, sel),
        SEP => ctx_separator(f, sel),
        REMOVE => ctx_remove(f, sel),
        UP | DOWN => ctx_move(f, sel, id == UP),
        RESET => ctx_reset(f),
        TEXT => {
            if let Some(open) = CTX_TEXT.with(|t| t.borrow().clone()) {
                open();
            }
            return;
        }
        _ => return,
    }
    commit_menu();
}

/// The size of the right-click menu editor's area in the settings window.
pub const CONTEXT_SIZE: (i32, i32) = (700, 480);

/// The right-click menu editor, built into a settings page. `all`: every command that can be offered; `current` /
/// `default`: the menu as a list of command names with "-" for a separator. `apply` gets the menu after each change;
/// `open_text` opens the file as text.
pub fn embed_context_menu(parent: &impl WxWidget, all: Vec<String>, current: Vec<String>, default: Vec<String>, apply: MenuApply, open_text: Rc<dyn Fn()>) {
    CTX.with(|c| *c.borrow_mut() = CtxState { all, menu: current, default, ..Default::default() });
    CTX_APPLY.with(|a| *a.borrow_mut() = Some(apply));
    CTX_TEXT.with(|t| *t.borrow_mut() = Some(open_text));
    form::on_edit(|f, id| {
        if id == CTX_FILTER {
            ctx_fill_avail(f);
        }
    });
    let spec = Spec {
        title: "Edit Popup ContextMenu",
        size: CONTEXT_SIZE,
        ctls: vec![
            ctl(110, Kind::Label, "Find command:", 0, 3, 90, 18),
            ctl(CTX_FILTER, Kind::Edit, "", 92, 0, 218, 22),
            ctl(CTX_AVAIL, Kind::MultiList, "", 0, 30, 310, 370),
            ctl(103, Kind::Button, "Add  >", 318, 138, 70, 28),
            ctl(104, Kind::Button, "<  Remove", 318, 174, 70, 28),
            ctl(CTX_MENU, Kind::List, "", 396, 30, 304, 370),
            ctl(105, Kind::Button, "Move Up", 396, 408, 76, 26),
            ctl(106, Kind::Button, "Move Down", 478, 408, 84, 26),
            ctl(107, Kind::Button, "Add Separator", 568, 408, 100, 26),
            ctl(108, Kind::Button, "Reset to default", 0, 408, 120, 26),
            ctl(109, Kind::Button, "Edit as text...", 126, 408, 110, 26),
            ctl(111, Kind::Label, "Select commands on the left and press Add; they go below the selected item on the right.", 0, 442, 700, 18),
        ],
        ok: "",
        buttons: None,
        on_change: Some(ctx_click),
        on_apply: None,
    };
    form::embed(parent, spec, |f| {
        ctx_fill_avail(f);
        ctx_fill_menu(f, Some(0));
    });
}
