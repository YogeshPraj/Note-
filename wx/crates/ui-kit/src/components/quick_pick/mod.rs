//! A type-to-filter list shown over the window: Command Palette, Quick Open, and any other "pick one of these".
//!
//! Give it items; it returns the id of the one the user chose. Filtering and keyboard navigation are in
//! [`filter`], which has no toolkit in it and is unit tested.
//!
//! ```ignore
//! let items = vec![Item { label: "File: New".into(), hint: "Ctrl+N".into(), id: 1001 }];
//! if let Some(id) = pick(&frame, items) { /* run command `id` */ }
//! ```

pub mod filter;

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::event::WindowEventData;
use wxdragon::prelude::*;

pub use self::filter::Item;
use self::filter::{Nav, navigate, rank};
use crate::controls::{Dialog, ListBox, TextBox};

const WIDTH: i32 = 640;
const ROW_H: i32 = 26;
const PAGE: i32 = 11;
const EDIT_H: i32 = 30;
/// Most rows kept for a query; more would make the list slow to fill.
const MAX_ROWS: usize = 300;

// wx key codes of the navigation keys.
const KEY_ESCAPE: i32 = 27;
const KEY_UP: i32 = 315;
const KEY_DOWN: i32 = 317;
const KEY_PAGE_UP: i32 = 366;
const KEY_PAGE_DOWN: i32 = 367;

/// Show the list over `frame`; the id of the chosen item, if any.
pub fn pick(frame: &Frame, items: Vec<Item>) -> Option<usize> {
    let d = Dialog::adopt(wxdragon::prelude::Dialog::builder(frame, "").with_style(DialogStyle::StayOnTop).build());
    let list_h = ROW_H * PAGE;
    d.set_client_size(Size::new(WIDTH, EDIT_H + list_h));
    let edit = TextBox::new(&d, "", true);
    edit.set_size_with_pos(0, 0, WIDTH, EDIT_H);
    let list = ListBox::new(&d);
    list.set_size_with_pos(0, EDIT_H, WIDTH, list_h);
    let (fp, fs) = (frame.get_position(), frame.get_size());
    d.move_window(fp.x + (fs.width - WIDTH) / 2, fp.y + 60);

    let items = Rc::new(items);
    // Indices into `items` of the rows currently listed.
    let shown: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let refilter = {
        let (items, shown) = (items.clone(), shown.clone());
        move || {
            let rows = rank(&items, &edit.get_value(), MAX_ROWS);
            list.clear();
            for i in &rows {
                list.append(&items[*i].row_text());
            }
            if !rows.is_empty() {
                list.set_selection(0, true);
            }
            *shown.borrow_mut() = rows;
        }
    };
    refilter();
    {
        let r = refilter.clone();
        edit.on_text_updated(move |_| r());
    }

    let chosen: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    let accept = {
        let (shown, chosen, items) = (shown.clone(), chosen.clone(), items.clone());
        move || {
            if let Some(i) = list.get_selection().and_then(|sel| shown.borrow().get(sel as usize).copied()) {
                *chosen.borrow_mut() = Some(items[i].id);
                d.end_modal(ID_OK);
            }
        }
    };
    {
        let a = accept.clone();
        edit.on_enter_pressed(move |_| a());
    }
    {
        let a = accept.clone();
        list.on_item_double_clicked(move |_| a());
    }
    edit.on_key_down(move |e| {
        let key = match &e {
            WindowEventData::Keyboard(k) => k.get_key_code().unwrap_or(0),
            _ => 0,
        };
        let nav = match key {
            KEY_UP => Some(Nav::Up),
            KEY_DOWN => Some(Nav::Down),
            KEY_PAGE_UP => Some(Nav::PageUp),
            KEY_PAGE_DOWN => Some(Nav::PageDown),
            _ => None,
        };
        match (nav, key) {
            (Some(n), _) => {
                let to = navigate(list.get_selection().map_or(-1, crate::num::to_i32), crate::num::to_i32(list.get_count()), PAGE, n);
                if to >= 0 {
                    list.set_selection(crate::num::to_u32(to), true);
                }
            }
            (None, KEY_ESCAPE) => d.end_modal(ID_CANCEL),
            _ => e.skip(true),
        }
    });
    edit.set_focus();
    d.show_modal();

    *chosen.borrow()
}
