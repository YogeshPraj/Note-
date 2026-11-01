//! Command pattern: every menu, toolbar and palette action is an id that the bus offers, in order,
//! to registered handlers. Adding a feature means writing a handler and registering it here;
//! nothing else has to change (open/closed).

use std::rc::Rc;

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