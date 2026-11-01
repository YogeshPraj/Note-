//! Our own versions of the stock controls: `Button`, `TextBox`, `CheckBox`, `Label`, `ListBox` and the rest.
//!
//! Application code uses these instead of wx's. Each one is a `Copy` handle that dereferences to the stock
//! widget (so every existing call and event hook still works), and each is *adopted* by the theme on creation:
//! it is coloured and natively styled now and re-styled whenever the theme changes, until it is destroyed.
//! This is the one place to change how controls look or behave application-wide.
//!
//! A control that needs a builder option the shortcut constructors lack builds the stock widget itself and
//! hands it over with `adopt`.

use std::ops::Deref;

use wxdragon::prelude::{Button as WxButton, CheckBox as WxCheckBox, Dialog as WxDialog, ListBox as WxListBox, ListCtrl as WxListCtrl, Panel as WxPanel, TreeCtrl as WxTreeCtrl, RadioButton as WxRadioButton, StaticText as WxStaticText, TextCtrl as WxTextCtrl, ToggleButton as WxToggleButton, *};
use wxdragon_sys as ffi;

use super::data_list;
use super::theme::{self, Role};

macro_rules! themed {
    ($(#[$doc:meta])* $name:ident, $inner:ty, $role:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy)]
        pub struct $name($inner);

        impl $name {
            /// Take over a stock widget: it follows the theme from now on.
            pub fn adopt(inner: $inner) -> Self {
                Self::adopt_as(inner, $role)
            }

            /// Like `adopt`, with a different role than the control's usual one.
            pub fn adopt_as(inner: $inner, role: Role) -> Self {
                theme::adopt(inner, role);
                Self(inner)
            }

            /// The stock widget underneath.
            pub fn inner(&self) -> $inner {
                self.0
            }
        }

        impl Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &$inner {
                &self.0
            }
        }

        impl WxWidget for $name {
            fn handle_ptr(&self) -> *mut ffi::wxd_Window_t {
                self.0.handle_ptr()
            }
            fn is_valid(&self) -> bool {
                self.0.is_valid()
            }
        }

        impl WxEvtHandler for $name {
            unsafe fn get_event_handler_ptr(&self) -> *mut ffi::wxd_EvtHandler_t {
                unsafe { self.0.get_event_handler_ptr() }
            }
        }
    };
}

themed!(
    /// A push button.
    Button, WxButton, Role::Surface
);
themed!(
    /// A button that stays pressed.
    ToggleButton, WxToggleButton, Role::Surface
);
themed!(
    /// A check box.
    CheckBox, WxCheckBox, Role::Surface
);
themed!(
    /// A radio button.
    RadioButton, WxRadioButton, Role::Surface
);
themed!(
    /// Static text.
    Label, WxStaticText, Role::Surface
);
themed!(
    /// A plain container.
    Panel, WxPanel, Role::Surface
);
themed!(
    /// A modal or modeless dialog: themed background and title bar.
    Dialog, WxDialog, Role::Surface
);
themed!(
    /// A text entry box (single or multi line).
    TextBox, WxTextCtrl, Role::Input
);
themed!(
    /// A simple list of strings.
    ListBox, WxListBox, Role::Input
);
themed!(
    /// A list with columns.
    ListView, WxListCtrl, Role::Input
);
themed!(
    /// A tree.
    TreeView, WxTreeCtrl, Role::Input
);

impl Button {
    pub fn new(parent: &impl WxWidget, label: &str) -> Button {
        Button::adopt(WxButton::builder(parent).with_label(label).build())
    }

    pub fn get_label(&self) -> String {
        self.0.get_label()
    }
}

impl ToggleButton {
    pub fn new(parent: &impl WxWidget, label: &str) -> ToggleButton {
        ToggleButton::adopt(WxToggleButton::builder(parent).with_label(label).build())
    }
}

impl CheckBox {
    pub fn new(parent: &impl WxWidget, label: &str) -> CheckBox {
        CheckBox::adopt(WxCheckBox::builder(parent).with_label(label).build())
    }
}

impl Label {
    pub fn new(parent: &impl WxWidget, text: &str) -> Label {
        Label::adopt(WxStaticText::builder(parent).with_label(text).build())
    }

    /// The stock static text's own `get_label` (a String); the window-level one would hide it.
    pub fn get_label(&self) -> String {
        self.0.get_label()
    }
}

impl Panel {
    pub fn new(parent: &impl WxWidget) -> Panel {
        Panel::adopt(WxPanel::builder(parent).build())
    }
}

impl Dialog {
    pub fn new(parent: &dyn WxWidget, title: &str) -> Dialog {
        Dialog::adopt(WxDialog::builder(parent, title).build())
    }
}

impl TextBox {
    /// A single-line box holding `value`; `enter` makes Enter raise an event instead of being swallowed.
    pub fn new(parent: &impl WxWidget, value: &str, enter: bool) -> TextBox {
        let b = WxTextCtrl::builder(parent).with_value(value);
        TextBox::adopt(if enter { b.with_style(TextCtrlStyle::ProcessEnter).build() } else { b.build() })
    }
}

impl ListBox {
    pub fn new(parent: &impl WxWidget) -> ListBox {
        ListBox::adopt(WxListBox::builder(parent).build())
    }
}

impl ListView {
    /// Set the columns, replacing any it had (the stock list cannot remove a single column).
    pub fn set_columns(&self, columns: &[(&str, i32)]) {
        for op in data_list::column_ops(crate::num::to_usize(self.0.get_column_count().max(0)), columns) {
            match op {
                data_list::ColumnOp::ClearAll => {
                    self.0.clear_all();
                }
                data_list::ColumnOp::Insert { index, name, width } => {
                    self.0.insert_column(crate::num::to_i64(index), &name, ListColumnFormat::Left, width);
                }
            }
        }
    }

    /// Replace all rows with `rows` (cells past the last column are ignored).
    pub fn fill(&self, rows: &[Vec<String>]) {
        self.0.delete_all_items();
        let columns = crate::num::to_usize(self.0.get_column_count().max(0));
        for op in data_list::cell_ops(rows, columns) {
            match op {
                data_list::CellOp::Insert { row, text } => {
                    self.0.insert_item(crate::num::to_i64(row), &text, None);
                }
                data_list::CellOp::Set { row, col, text } => self.0.set_item_text_by_column(crate::num::to_i64(row), crate::num::to_i32(col), &text),
            }
        }
    }

    /// The first selected row.
    pub fn selected(&self) -> Option<usize> {
        usize::try_from(self.0.get_first_selected_item()).ok()
    }

    /// Every selected row, in order.
    pub fn selected_all(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = -1i64;
        loop {
            let n = self.0.get_next_item(i, ListNextItemFlag::All, ListItemState::Selected);
            if n < 0 {
                break;
            }
            out.push(crate::num::to_usize(n));
            i = i64::from(n);
        }
        out
    }
}