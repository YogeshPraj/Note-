//! Command pattern: every menu, toolbar and palette action is an id that the bus offers, in order,
//! to registered handlers. Adding a feature means writing a handler and registering it here;
//! nothing else has to change (open/closed).

use std::rc::Rc;

use notepp_core::registry::{self, CommandState};

use crate::app::App;
use crate::extra::ExtraCommands;
use crate::handlers::{AppearanceCommands, HelpCommands, ViewCommands};
use crate::panels::PanelCommands;
use crate::streams::StreamCommands;
use crate::updates::UpdateCommands;

/// One cohesive group of commands acting on a context `C` (the application, or a fake in tests).
pub trait CommandHandler<C> {
    /// Used by tracing and diagnostics.
    fn name(&self) -> &'static str;
    /// Run command `id` if this handler owns it. False lets the next handler try.
    fn handle(&self, ctx: &mut C, id: usize) -> bool;
}

/// Ordered chain of responsibility over the registered handlers.
pub struct CommandBus<C> {
    handlers: Vec<Box<dyn CommandHandler<C>>>,
}

impl<C> Default for CommandBus<C> {
    fn default() -> Self {
        Self { handlers: Vec::new() }
    }
}

impl<C> CommandBus<C> {
    pub fn register(mut self, h: impl CommandHandler<C> + 'static) -> Self {
        self.handlers.push(Box::new(h));
        self
    }

    /// Name of the handler that ran the command, if any did.
    pub fn dispatch(&self, ctx: &mut C, id: usize) -> Option<&'static str> {
        self.handlers.iter().find(|h| h.handle(ctx, id)).map(|h| h.name())
    }
}

/// Why a command did not run. The text is what the status bar shows.
#[derive(Debug)]
pub enum AppError {
    /// The command exists but its enabled rule says not now (nothing selected, nothing to undo, ...).
    Disabled(String),
    /// The command has no handler in this build.
    NotAvailable,
    Core(notepp_core::error::Error),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Disabled(name) => write!(f, "{name} is not available right now"),
            AppError::NotAvailable => f.write_str("This command is not available in this build yet"),
            AppError::Core(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for AppError {}

impl From<notepp_core::error::Error> for AppError {
    fn from(e: notepp_core::error::Error) -> Self {
        AppError::Core(e)
    }
}

/// The id span that holds every text command, fixed or in a family (see [`registry::needs_text`]).
const TEXT_IDS: (usize, usize) = (2000, 9000);

/// Run a command by id. Everything that can start an action goes through here: it checks the command's enabled
/// rule from the registry, then hands it to the core and the registered handlers.
pub fn execute(app: &mut App, id: usize) -> Result<(), AppError> {
    let state = CommandState::of(&app.ws);
    if !registry::is_enabled(id, state) {
        let name = registry::spec(id).map_or_else(String::new, notepp_core::registry::CommandSpec::tooltip);
        return Err(AppError::Disabled(name));
    }
    app.dispatch_command(id)
}
impl App {
    /// Enable or grey the menu items and toolbar buttons whose registry rule depends on the editor's state.
    pub fn sync_command_state(&mut self) {
        let state = CommandState::of(&self.ws);
        if self.cmd_state == Some(state) {
            return;
        }
        // Switching between a whiteboard and a text tab greys or restores every text command, families included.
        let tab_kind_changed = self.cmd_state.is_none_or(|old| old.whiteboard != state.whiteboard);
        self.cmd_state = Some(state);
        for spec in registry::state_dependent() {
            let on = registry::is_enabled(spec.id, state);
            self.menubar.enable_item(crate::platform::wx::to_i32(spec.id), on);
            self.chrome.set_enabled(spec.id, on);
        }
        if tab_kind_changed {
            for id in (TEXT_IDS.0..TEXT_IDS.1).filter(|&id| registry::needs_text(id)) {
                let wx_id = crate::platform::wx::to_i32(id);
                if self.menubar.find_item(wx_id).is_some() {
                    self.menubar.enable_item(wx_id, registry::is_enabled(id, state));
                }
                self.chrome.set_enabled(id, registry::is_enabled(id, state));
            }
        }
    }
}
impl CommandBus<App> {
    /// The handlers of the shipped application, in priority order.
    pub fn standard() -> Rc<Self> {
        Rc::new(
            Self::default()
                .register(ViewCommands)
                .register(PanelCommands)
                .register(UpdateCommands)
                .register(StreamCommands)
                .register(ExtraCommands)
                .register(crate::paste_special::PasteSpecialCommands)
                .register(AppearanceCommands)
                .register(HelpCommands),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Ctx {
        log: Vec<String>,
    }

    struct Owns(&'static str, &'static [usize]);

    impl CommandHandler<Ctx> for Owns {
        fn name(&self) -> &'static str {
            self.0
        }
        fn handle(&self, ctx: &mut Ctx, id: usize) -> bool {
            let mine = self.1.contains(&id);
            if mine {
                ctx.log.push(format!("{}:{id}", self.0));
            }
            mine
        }
    }

    #[test]
    fn first_owner_runs_and_stops_the_chain() {
        let bus = CommandBus::default().register(Owns("a", &[1, 2])).register(Owns("b", &[2, 3]));
        let mut ctx = Ctx::default();
        assert_eq!(bus.dispatch(&mut ctx, 2), Some("a"));
        assert_eq!(ctx.log, ["a:2"]);
    }

    #[test]
    fn falls_through_to_a_later_handler() {
        let bus = CommandBus::default().register(Owns("a", &[1])).register(Owns("b", &[3]));
        let mut ctx = Ctx::default();
        assert_eq!(bus.dispatch(&mut ctx, 3), Some("b"));
    }

    #[test]
    fn unknown_command_reports_none_and_changes_nothing() {
        let bus = CommandBus::default().register(Owns("a", &[1]));
        let mut ctx = Ctx::default();
        assert_eq!(bus.dispatch(&mut ctx, 99), None);
        assert_eq!(ctx.log, [] as [std::string::String; 0]);
    }
}
