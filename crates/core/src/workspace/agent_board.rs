//! The agent tools for whiteboards. A whiteboard's latest strokes are in its page, so the page does the work: the workspace
//! checks the call and brings the tab to the front, and the window hands the request to the page and answers when it replies.

use serde_json::{Value, json};

use super::agent_commands::needs_more;
use super::*;
use crate::error::Error;
use crate::mcp::access::Level;
use crate::mcp::args::Args;
use crate::mcp::tools;

/// A request for the page showing tab `tab`.
#[derive(Debug, PartialEq)]
pub struct BoardRequest {
    pub tab: u64,
    /// What the page is to do.
    pub op: Value,
    /// When the page has answered, save the tab: the answer brings its latest scene.
    pub save_after: bool,
}

impl<S: Send> Workspace<S> {
    /// The whiteboard tab an agent named.
    fn agent_board_tab(&self, args: Args<'_>) -> Result<usize, Error> {
        let i = self.agent_tab(args)?;
        let t = &self.tabs[i];
        if t.kind != TabKind::Whiteboard {
            return Err(Error::Agent(format!("\"{}\" is not a whiteboard.", t.title)));
        }
        if t.encrypted {
            return Err(Error::Agent(format!("\"{}\" is encrypted, and agents cannot use encrypted documents.", t.title)));
        }
        Ok(i)
    }

    /// What the page has to do for this tool call, or None when it is not for a page. The tab comes to the front, as a page
    /// only holds the tab it shows.
    ///
    /// # Errors
    /// Why the call cannot run, before anything has moved.
    pub fn agent_board(&mut self, level: Level, tool: &str, args: &Value) -> Result<Option<BoardRequest>, Error> {
        let a = Args(args);
        let saving = tool == "save_document" && self.agent_tab(a).is_ok_and(|i| self.tabs[i].kind == TabKind::Whiteboard);
        let Some(spec) = tools::find(tool).filter(|_| saving || tools::is_board(tool)) else { return Ok(None) };
        if spec.level > level {
            return Err(needs_more(tool, spec.level));
        }
        let i = self.agent_board_tab(a)?;
        if saving {
            self.agent_save_target(i, a)?;
        }
        self.activate(i);
        let op = json!({ "tool": if saving { "sync" } else { tool }, "args": args });
        Ok(Some(BoardRequest { tab: self.tabs[i].id, op, save_after: saving }))
    }

    pub(super) fn agent_new_board(&mut self) -> Value {
        let i = self.new_whiteboard();
        json!({ "tab": self.tabs[i].id, "title": self.tabs[i].title })
    }
}
