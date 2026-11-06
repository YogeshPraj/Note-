//! The agent tools for whiteboards and pictures. Their latest state is in their pages, so the page does the work: the
//! workspace checks the call and brings the tab to the front, and the window hands the request to the page and answers when
//! it replies.

use serde_json::{Value, json};

use super::agent_commands::needs_more;
use super::*;
use crate::error::Error;
use crate::mcp::access::Level;
use crate::mcp::args::Args;
use crate::mcp::{page_tools, tools};

/// The limits of a picture, as in `assets/imageagent.js`.
const MAX_PICTURE_SIDE: u64 = 16_000;
const MAX_PICTURE_AREA: u64 = 64_000_000;

/// A request for the page showing tab `tab`.
#[derive(Debug, PartialEq)]
pub struct BoardRequest {
    pub tab: u64,
    /// What the page is to do.
    pub op: Value,
    /// When the page has answered, save the tab: the answer brings its latest scene.
    pub save_after: bool,
}

/// What the page would refuse about a new picture, checked before a tab exists that would then stay empty.
fn check_new_picture(args: Args<'_>) -> Result<(), Error> {
    let side = |name: &str| {
        args.number(name)?
            .filter(|n| (1..=MAX_PICTURE_SIDE).contains(n))
            .ok_or_else(|| Error::Agent(format!("\"{name}\" must be from 1 to {MAX_PICTURE_SIDE}.")))
    };
    let (width, height) = (side("width")?, side("height")?);
    if width.saturating_mul(height) > MAX_PICTURE_AREA {
        return Err(Error::Agent("That size is more than a picture can be.".into()));
    }
    args.opt_text("background")?;
    Ok(())
}

impl<S: Send> Workspace<S> {
    /// The tab of the kind `kind` an agent named.
    fn agent_page_tab(&self, args: Args<'_>, kind: TabKind) -> Result<usize, Error> {
        let i = self.agent_tab(args)?;
        let t = &self.tabs[i];
        if t.kind != kind {
            return Err(Error::Agent(format!("\"{}\" is not a {}.", t.title, super::agent::kind_name(kind))));
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
        let named = self.agent_tab(a).ok().map(|i| self.tabs[i].kind).filter(|k| *k != TabKind::Text);
        let saving = tool == "save_document" && named.is_some();
        let Some(kind) = page_tools::kind_of(tool).or(named.filter(|_| saving)) else { return Ok(None) };
        let Some(spec) = tools::find(tool) else { return Ok(None) };
        if spec.level > level {
            return Err(needs_more(tool, spec.level));
        }
        if tool == "new_picture" {
            check_new_picture(a)?;
            let i = self.new_image(String::new());
            return Ok(Some(BoardRequest { tab: self.tabs[i].id, op: json!({ "tool": tool, "args": args }), save_after: false }));
        }
        let i = self.agent_page_tab(a, kind)?;
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
