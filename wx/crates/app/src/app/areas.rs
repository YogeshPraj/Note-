//! Moving the keyboard between the window's areas with F6 and Shift+F6: the editor (Tab types there), the toolbar, the
//! tabs and the status bar. Inside a bar, `keynav` takes over. Hidden and empty bars are passed over.

use notepp_core::commands as c;

use super::*;
use crate::command::CommandHandler;
use crate::widgets::keynav::Exit;

/// The editor and the three bars.
const AREAS: usize = 4;

/// The area `step` places on from `from`, going forward or back; 0 is the editor.
fn area_after(from: usize, step: usize, forward: bool) -> usize {
    (from + if forward { step } else { AREAS - step % AREAS }) % AREAS
}

impl App {
    /// Give the keyboard to what the active tab shows.
    fn focus_editor(&self) {
        if self.ws.is_whiteboard() {
            self.wbv.set_focus();
        } else {
            self.stc().set_focus();
        }
    }

    fn move_area(&self, forward: bool) {
        let from = self.chrome.bar_in_use().map_or(0, |bar| bar + 1);
        for step in 1..=AREAS {
            let to = area_after(from, step, forward);
            if to == 0 {
                return self.focus_editor();
            }
            if self.chrome.enter_bar(to - 1) {
                return;
            }
        }
    }
}

/// View: Next Area and Previous Area.
pub struct AreaCommands;

impl CommandHandler<App> for AreaCommands {
    fn name(&self) -> &'static str {
        "areas"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_NEXT_AREA => app.move_area(true),
            c::ID_PREV_AREA => app.move_area(false),
            _ => return false,
        }
        true
    }
}

/// The command a shortcut means. Tab and Shift+Tab are editor shortcuts, which fire before a bar sees the key; while a bar
/// has the keyboard they move on, and must not change text the user is not in.
pub(super) fn in_bar(app: &Shared, id: usize) -> usize {
    let in_bar = app.try_borrow().is_ok_and(|a| a.chrome.bar_in_use().is_some());
    bar_command(id, in_bar)
}

fn bar_command(id: usize, in_bar: bool) -> usize {
    match id {
        c::ID_INDENT_INC if in_bar => c::ID_NEXT_AREA,
        c::ID_INDENT_DEC if in_bar => c::ID_PREV_AREA,
        _ => id,
    }
}

/// Tab and Esc inside a bar.
pub(super) fn wire_areas(app: &Shared) {
    let a = app.clone();
    let leave: Rc<dyn Fn(Exit)> = Rc::new(move |exit| match exit {
        Exit::Next => run_command(&a, c::ID_NEXT_AREA),
        Exit::Previous => run_command(&a, c::ID_PREV_AREA),
        Exit::Editor => {
            if let Ok(app) = a.try_borrow() {
                app.focus_editor();
            }
        }
    });
    app.borrow().chrome.set_bar_exit(&leave);
}

#[cfg(test)]
mod tests {
    use super::{area_after, bar_command, c};

    #[test]
    fn tab_moves_on_inside_a_bar_and_indents_in_the_editor() {
        assert_eq!(bar_command(c::ID_INDENT_INC, true), c::ID_NEXT_AREA);
        assert_eq!(bar_command(c::ID_INDENT_DEC, true), c::ID_PREV_AREA);
        assert_eq!(bar_command(c::ID_INDENT_INC, false), c::ID_INDENT_INC);
        assert_eq!(bar_command(c::ID_SAVE, true), c::ID_SAVE);
    }

    #[test]
    fn forward_goes_round_through_the_bars_to_the_editor() {
        assert_eq!(area_after(0, 1, true), 1);
        assert_eq!(area_after(3, 1, true), 0);
    }

    #[test]
    fn back_goes_the_other_way_round() {
        assert_eq!(area_after(0, 1, false), 3);
        assert_eq!(area_after(1, 1, false), 0);
    }

    #[test]
    fn a_hidden_bar_is_stepped_over() {
        assert_eq!(area_after(0, 2, true), 2);
        assert_eq!(area_after(0, 2, false), 2);
    }
}
