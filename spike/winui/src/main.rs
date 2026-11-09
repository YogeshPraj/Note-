//! Phase 2 spike (docs/winui3-plan.md): stock WinUI 3 controls through windows-reactor, Scintilla inside, files read
//! and menus described by `notepp-core`. Not shipped.
//!
//! `NOTEPP_SPIKE_REPORT=<file>`: append what happened (window found, editor placed, keys routed) to that file.
#![windows_subsystem = "windows"]

mod host;
mod keys;
mod menu_open;

use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use notepp_core::commands::{ID_COPY, ID_CUT, ID_NEW, ID_PASTE, ID_REDO, ID_SAVE, ID_SELALL, ID_UNDO};
use notepp_core::key_routing::Route;
use notepp_core::keymap::KeyMap;
use notepp_core::menu::{Entry, MenuSpec};
use windows_reactor::*;

fn report(line: &str) {
    if let Ok(path) = std::env::var("NOTEPP_SPIKE_REPORT")
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// The menu bar as `core` data: (title with its `&` mark, menu).
fn menus() -> Vec<(String, MenuSpec)> {
    let item = |m: MenuSpec, id: usize| {
        let label = notepp_core::registry::spec(id).map_or_else(|| format!("#{id}"), |s| s.menu.to_string());
        m.item(id, &label)
    };
    let file = [ID_NEW, ID_SAVE].into_iter().fold(MenuSpec::new(), item);
    let edit = item(item(MenuSpec::new(), ID_UNDO), ID_REDO).sep();
    let edit = [ID_CUT, ID_COPY, ID_PASTE, ID_SELALL].into_iter().fold(edit, item);
    vec![("&File".into(), file), ("&Edit".into(), edit)]
}

/// WinUI menu items for a `core` menu. A click reports the item's label, so labels map back to command ids.
fn menu_items(spec: &MenuSpec, ids: &mut HashMap<String, usize>) -> Vec<MenuItem> {
    spec.entries()
        .iter()
        .enumerate()
        .map(|(n, e)| match e {
            Entry::Item { id, label, .. } => {
                let text = label.replace('&', "");
                ids.insert(text.clone(), *id);
                MenuItem::item(*id, text)
            }
            Entry::Sep => MenuItem::separator(format!("sep{n}")),
            Entry::Sub { label, menu } => MenuItem::Submenu { key: label.as_str().into(), label: label.replace('&', ""), items: menu_items(menu, ids) },
        })
        .collect()
}

struct Doc {
    name: String,
    path: Option<std::path::PathBuf>,
    text: String,
}

#[derive(Clone)]
enum Msg {
    Select(Option<usize>),
    /// The tab strip catches up with the selection (a tab added and selected in one turn is not selected by WinUI).
    ShowTab,
    Menu(String),
    Key(keys::Routed),
    Placed(String),
}

struct Spike {
    docs: Vec<Doc>,
    selected: usize,
    /// The tab the strip shows; follows `selected` one turn late when a tab was just added.
    tab_shown: usize,
    status: String,
    panel: ElementRef<SwapChainPanel>,
    _surface: ElementObservation,
}

fn open_docs() -> Vec<Doc> {
    let mut docs: Vec<Doc> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter_map(|p| {
            let bytes = std::fs::read(&p).ok()?;
            let (text, _, _) = notepp_core::doc::decode(&bytes);
            Some(Doc { name: p.file_name()?.to_string_lossy().into_owned(), path: Some(p), text })
        })
        .collect();
    if docs.is_empty() {
        docs.push(Doc { name: "new 1".into(), path: None, text: "Scintilla inside WinUI 3.\r\n".into() });
    }
    docs
}

impl Spike {
    fn show(&mut self, i: usize) {
        if i == self.selected || i >= self.docs.len() {
            return;
        }
        self.docs[self.selected].text = host::text();
        self.selected = i;
        host::set_text(&self.docs[i].text);
        host::focus_editor();
    }

    fn run(&mut self, id: usize) {
        const SCI: [(usize, u32); 6] = [(ID_UNDO, 2176), (ID_REDO, 2011), (ID_CUT, 2177), (ID_COPY, 2178), (ID_PASTE, 2179), (ID_SELALL, 2013)];
        if let Some((_, msg)) = SCI.iter().find(|(c, _)| *c == id) {
            host::command(*msg);
        } else if id == ID_NEW {
            self.docs[self.selected].text = host::text();
            self.docs.push(Doc { name: format!("new {}", self.docs.len() + 1), path: None, text: String::new() });
            self.selected = self.docs.len() - 1;
            host::set_text("");
        } else if id == ID_SAVE {
            let doc = &self.docs[self.selected];
            self.status = match &doc.path {
                Some(p) => std::fs::write(p, host::text()).map_or_else(|e| format!("save failed: {e}"), |()| format!("saved {}", doc.name)),
                None => "Save: no path (Save As is not in the spike)".into(),
            };
            return;
        }
        self.status = format!("ran command {id}");
        host::focus_editor();
    }

    fn update_inner(&mut self, msg: Msg) {
        match msg {
            Msg::ShowTab => report(&format!("show tab {} of {}", self.selected, self.docs.len())),
            Msg::Select(Some(i)) => {
                report(&format!("tab strip selected {i}"));
                self.show(i);
            }
            Msg::Select(None) => report("tab strip selected none"),
            Msg::Menu(label) => match IDS.with(|t| t.borrow().get(&label).copied()) {
                Some(id) => self.run(id),
                None => self.status = format!("menu item {label:?} has no command"),
            },
            Msg::Key(k) => self.key(k),
            Msg::Placed(line) => {
                report(&line);
                if let Some((w, e)) = host::handles() {
                    report(&format!("window {:#x} editor {:#x}; children (top first) {:?}", host::hwnd_value(w), host::hwnd_value(e), host::children()));
                }
                self.status = line;
            }
        }
    }

    fn key(&mut self, k: keys::Routed) {
        report(&format!("key {} -> {:?}", k.press, k.route));
        match k.route {
            Route::SwitchTab(d) => {
                let n = self.docs.len();
                let next = (self.selected + n).wrapping_add_signed(isize::try_from(d).unwrap_or(0)) % n;
                self.show(next);
            }
            Route::Command(id) => self.run(id),
            Route::MenuLetter(c) => open_menu(keys::title_for(c)),
            Route::FirstMenu => open_menu(keys::first_title()),
            _ => {}
        }
    }
}

impl Component for Spike {
    type Input = ();
    type Message = Msg;

    fn create(_: &(), context: &ComponentContext<Self>) -> Self {
        let docs = open_docs();
        let first = docs[0].text.clone();
        let placed = context.sender().callback(Msg::Placed);
        let panel = ElementRef::<SwapChainPanel>::new();
        let surface = panel.observe_surface(move |e| {
            if let SwapChainPanelEvent::Metrics { width, height, scale_x, .. } = e {
                let fresh = !host::is_ready();
                let line = host::place(width, height, scale_x).unwrap_or_else(|e| format!("place failed: {e}"));
                if fresh && host::is_ready() {
                    host::set_text(&first);
                    host::focus_editor();
                }
                let _ = placed.call(line);
            }
        });
        let titles = menus().into_iter().map(|(t, _)| t).collect();
        let keys = context.sender().callback(Msg::Key);
        let hooked = keys::install(KeyMap::build(&BTreeMap::new()), titles, move |k| {
            let _ = keys.call(k);
        });
        report(&format!("key hook installed: {hooked}"));
        report("created");
        Self { docs, selected: 0, tab_shown: 0, status: String::new(), panel, _surface: surface }
    }

    fn update(&mut self, msg: Msg, context: &ComponentContext<Self>) {
        let tabs = self.docs.len();
        self.update_inner(msg);
        if self.docs.len() == tabs {
            self.tab_shown = self.selected;
        } else {
            // Queued messages are handled before the next publish, so the selection must wait for a later one.
            let _ = context.spawn_background(|_| {
                std::thread::sleep(std::time::Duration::from_millis(30));
                Msg::ShowTab
            });
        }
    }

    fn view(&self, _: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title(format!("{} - Note++ WinUI spike", self.docs[self.selected].name));
        let mut ids = HashMap::new();
        let bar = MenuBar::new().collection_slot(
            MenuBarSlot::Items,
            menus().iter().map(|(title, spec)| {
                let items = menu_items(spec, &mut ids);
                (title.as_str(), MenuBarItem::new().title(title.replace('&', "")).menu(Menu::new(items, context.callback(Msg::Menu))))
            }),
        );
        // A menu click reports a label; this table, rebuilt with each view, turns it back into a command id.
        IDS.with(|t| *t.borrow_mut() = ids);
        let tabs = TabView::new()
            .selected_index(Some(self.tab_shown))
            .is_add_tab_button_visible(false)
            .on_selection_changed(context.callback(Msg::Select))
            .grid_row(1)
            .collection_slot(
                TabViewSlot::TabItems,
                self.docs.iter().enumerate().map(|(i, d)| (i, TabViewItem::new().header(d.name.as_str()).is_closable(false))),
            );
        Grid::new().rows([GridLength::Auto, GridLength::Auto, GridLength::Auto, GridLength::STAR]).children((
            bar,
            tabs,
            TextBlock::new().text(self.status.as_str()).margin(Thickness::new(8.0, 2.0, 8.0, 2.0)).grid_row(2),
            SwapChainPanel::new().element_ref(&self.panel).grid_row(3),
        ))
    }
}

/// Open a menu bar item by its title (UI Automation, on a worker thread); the outcome goes to the report.
fn open_menu(title: Option<String>) {
    let (Some(title), Some((window, _))) = (title, host::handles()) else {
        report("open menu: no title or no window");
        return;
    };
    menu_open::expand(host::hwnd_value(window), title, |outcome| report(&format!("open menu: {outcome}")));
}

thread_local! {
    static IDS: std::cell::RefCell<HashMap<String, usize>> = std::cell::RefCell::new(HashMap::new());
}

fn main() {
    report(&format!("start {}", env!("CARGO_PKG_VERSION")));
    if let Err(e) = App::run_component::<Spike>(()) {
        report(&format!("run failed: {e:?}"));
    }
    report("exit");
}
