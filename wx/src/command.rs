//! Command pattern: every menu, toolbar and palette action is an id that the bus offers, in order,
//! to registered handlers. Adding a feature means writing a handler and registering it here;
//! nothing else has to change (open/closed).

use std::rc::Rc;

use crate::app::App;
use crate::handlers::{AppearanceCommands, HelpCommands, ViewCommands};

/// One cohesive group of commands.
pub trait CommandHandler {
    /// Used by tracing and diagnostics.
    fn name(&self) -> &'static str;
    /// Run command `id` if this handler owns it. False lets the next handler try.
    fn handle(&self, app: &mut App, id: usize) -> bool;
}

/// Ordered chain of responsibility over the registered handlers.
#[derive(Default)]
pub struct CommandBus {
    handlers: Vec<Box<dyn CommandHandler>>,
}

impl CommandBus {
    pub fn register(mut self, h: impl CommandHandler + 'static) -> Self {
        self.handlers.push(Box::new(h));
        self
    }

    /// Name of the handler that ran the command, if any did.
    pub fn dispatch(&self, app: &mut App, id: usize) -> Option<&'static str> {
        self.handlers.iter().find(|h| h.handle(app, id)).map(|h| h.name())
    }

    /// The handlers of the shipped application, in priority order.
    pub fn standard() -> Rc<Self> {
        Rc::new(
            Self::default()
                .register(ViewCommands)
                .register(PanelCommands)
                .register(UpdateCommands)
                .register(StreamCommands)
                .register(ExtraCommands)
                .register(AppearanceCommands)
                .register(HelpCommands),
        )
    }
}

/// Adapters for the feature modules that already own their command logic.
macro_rules! delegate {
    ($ty:ident, $name:literal, $method:ident) => {
        pub struct $ty;
        impl CommandHandler for $ty {
            fn name(&self) -> &'static str {
                $name
            }
            fn handle(&self, app: &mut App, id: usize) -> bool {
                app.$method(id)
            }
        }
    };
}

delegate!(PanelCommands, "panels", panel_cmd);
delegate!(UpdateCommands, "updates", update_cmd);
delegate!(StreamCommands, "streams", stream_cmd);
delegate!(ExtraCommands, "extra", extra_command);
