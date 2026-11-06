//! Running registry commands for an agent. The workspace decides whether a command may run and prepares for it; the window
//! runs it, since many commands are handled there.

use serde_json::{Value, json};

use super::*;
use crate::error::Error;
use crate::mcp::access::Level;
use crate::mcp::args::Args;
use crate::mcp::commands;

/// The words for a tool or command that needs more than the user allows.
pub(super) fn needs_more(what: &str, needed: Level) -> Error {
    Error::Agent(format!("{what} needs the level {}. Ask the user to choose it under Settings > AI Agents.", needed.label()))
}

impl<S: Send> Workspace<S> {
    /// Whether the command an agent named may run now, which brings the tab it named to the front. Gives its id, for the
    /// window to run.
    ///
    /// # Errors
    /// Why not: an unknown or forbidden command, too low a level, or a tab it must not work on.
    pub fn agent_command(&mut self, level: Level, args: &Value) -> Result<usize, Error> {
        let args = Args(args);
        let name = args.text("command")?;
        let (spec, needed) = commands::find(name).ok_or_else(|| Error::Agent(format!("Agents cannot run \"{name}\". Call list_commands.")))?;
        if needed > level {
            return Err(needs_more(spec.palette, needed));
        }
        let i = self.agent_tab(args)?;
        if commands::changes_text(spec.id) {
            self.agent_text_tab(args)?;
            if self.with_doc(self.tabs[i].doc, |s| s.send(sci::SCI_GETREADONLY, 0, 0)) != 0 {
                return Err(Error::Agent(format!("\"{}\" is read-only.", self.tabs[i].title)));
            }
        }
        self.activate(i);
        // Closing a modified tab would ask the user.
        if spec.id == crate::commands::ID_CLOSE && self.is_dirty(i) {
            return Err(Error::Agent(format!("\"{}\" has unsaved changes. Save it first, or ask the user.", self.tabs[i].title)));
        }
        Ok(spec.id)
    }

    /// Mark where a command starts or ends, so that a text command is one undo step.
    pub fn agent_undo_group(&self, id: usize, start: bool) {
        if commands::one_undo_step(id) {
            self.send(if start { sci::SCI_BEGINUNDOACTION } else { sci::SCI_ENDUNDOACTION }, 0, 0);
        }
    }

    /// Bring a tab to the front and select whole lines, for the commands that work on a selection.
    pub(super) fn agent_select(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let first = crate::num::to_usize(args.number("start_line")?.ok_or_else(|| Error::Agent("\"start_line\" is required.".into()))?.max(1));
        let last = crate::num::to_usize(args.number("end_line")?.unwrap_or(0)).max(first);
        self.activate(i);
        let end_of_document = crate::num::to_usize(sci::length(&self.sci).max(0));
        let start_of = |line: usize| match self.send(sci::SCI_POSITIONFROMLINE, line - 1, 0) {
            p if p < 0 => end_of_document,
            p => crate::num::to_usize(p),
        };
        search::select_range(&self.sci, start_of(first), start_of(last + 1));
        Ok(json!({ "tab": self.tabs[i].id, "first_line": first, "last_line": last }))
    }
}
