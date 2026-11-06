//! What an AI agent can do in the workspace: the tools of `mcp::tools`.
//!
//! Tabs are reached through `with_doc`, so working on a tab does not move the user off the one in front. Nothing here asks
//! the user anything: [`Headless`] stands where the dialogs are, and what would have been a question is a refusal.

use std::cell::RefCell;

use serde_json::{Value, json};

use super::*;
use crate::error::Error;
use crate::mcp::access::{Level, Reach};
use crate::mcp::args::Args;
use crate::mcp::{text, tools};

/// The most a single read gives, so a huge document cannot swamp the agent: it reads on from `next_line`.
const MAX_READ: usize = 256 * 1024;
/// The largest file an agent can open: reading one blocks the window.
const MAX_OPEN: u64 = 64 * 1024 * 1024;
/// The most matches `find_text` lists, and how much of each line.
const MAX_HITS: usize = 200;
const MAX_HIT_LINE: usize = 300;

/// In place of the dialogs while an agent is working: every question is answered no, and an error is kept to pass on.
#[derive(Debug, Default)]
pub(super) struct Headless {
    said: RefCell<Option<String>>,
}

impl Headless {
    /// What went wrong, for the agent.
    pub(super) fn failure(&self, otherwise: &str) -> Error {
        Error::Agent(self.said.borrow_mut().take().unwrap_or_else(|| otherwise.to_string()))
    }
}

impl Ui for Headless {
    fn pick_open(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_save(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn confirm_save(&self, _: &str) -> Confirm {
        Confirm::Cancel
    }
    fn ask_yes_no(&self, _: &str) -> bool {
        false
    }
    fn recycle(&self, _: &Path) -> Result<(), Error> {
        Err(Error::Agent("Agents cannot delete files.".into()))
    }
    fn error(&self, msg: &str) {
        *self.said.borrow_mut() = Some(msg.to_string());
    }
    fn unlock(&self, _: &mut crypto::Vault, _: &str) -> bool {
        false
    }
    fn setup_encryption(&self, _: &mut crypto::Vault) -> bool {
        false
    }
}

fn kind_name(kind: TabKind) -> &'static str {
    match kind {
        TabKind::Text => "text",
        TabKind::Whiteboard => "whiteboard",
        TabKind::Image => "picture",
    }
}

impl<S: Send> Workspace<S> {
    /// Carry out one tool call. The caller is on the UI thread; `level` is what the user allows now.
    ///
    /// # Errors
    /// Why the tool did not run, in words the agent can act on or pass on to the user.
    pub fn agent_call(&mut self, level: Level, tool: &str, args: &Value) -> Result<Value, Error> {
        let spec = tools::find(tool).ok_or_else(|| Error::Agent(format!("There is no tool \"{tool}\".")))?;
        if spec.level > level {
            return Err(Error::Agent(format!("Note++ only lets agents read. Ask the user to choose Edit under Settings > AI Agents to use {tool}.")));
        }
        let args = Args(args);
        match tool {
            "list_tabs" => Ok(self.agent_list()),
            "read_document" => self.agent_read(args),
            "find_text" => self.agent_find(args),
            "open_file" => self.agent_open(args),
            "switch_tab" => self.agent_switch(args),
            _ => self.agent_change(tool, args),
        }
    }

    /// The tab an agent named, or the active one.
    pub(super) fn agent_tab(&self, args: Args<'_>) -> Result<usize, Error> {
        match args.number("tab")? {
            None => Ok(self.active),
            Some(id) => self.tabs.iter().position(|t| t.id == id).ok_or_else(|| Error::Agent(format!("No tab has the id {id}. Call list_tabs."))),
        }
    }

    /// As `agent_tab`, for a tool that needs the text. Encrypted documents stay out of an agent's reach.
    pub(super) fn agent_text_tab(&self, args: Args<'_>) -> Result<usize, Error> {
        let i = self.agent_tab(args)?;
        let t = &self.tabs[i];
        if t.kind != TabKind::Text {
            return Err(Error::Agent(format!("\"{}\" is a {}, and agents can only work on text for now.", t.title, kind_name(t.kind))));
        }
        if t.encrypted {
            return Err(Error::Agent(format!("\"{}\" is encrypted, and agents cannot use encrypted documents.", t.title)));
        }
        Ok(i)
    }

    /// What an agent may reach on disk.
    pub(super) fn agent_reach<'a>(&'a self, profile: &'a Path) -> Reach<'a> {
        Reach { workspace: self.workspace.as_deref(), open: self.tabs.iter().filter_map(|t| t.path.as_deref()).collect(), profile }
    }

    fn agent_list(&self) -> Value {
        let rows: Vec<Value> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut row = json!({
                    "id": t.id,
                    "title": t.title,
                    "path": t.path,
                    "kind": kind_name(t.kind),
                    "modified": self.is_dirty(i),
                    "active": i == self.active,
                    "encrypted": t.encrypted,
                });
                if t.kind == TabKind::Text && !t.encrypted {
                    row["lines"] = json!(self.with_doc(t.doc, |s| sci::line_count(s)));
                    row["language"] = json!(t.lang_label);
                }
                row
            })
            .collect();
        json!({ "tabs": rows, "workspace_folder": self.workspace })
    }

    fn agent_read(&self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let start = crate::num::to_usize(args.number("start_line")?.unwrap_or(1));
        let count = args.number("line_count")?.map(crate::num::to_usize);
        let all = self.with_doc(self.tabs[i].doc, |s| sci::get_text(s));
        let part = text::lines(&all, start, count, MAX_READ);
        Ok(json!({
            "tab": self.tabs[i].id,
            "title": self.tabs[i].title,
            "first_line": part.first,
            "last_line": part.last,
            "total_lines": part.total,
            "next_line": part.next,
            "line_cut_short": part.cut,
            "text": part.text,
        }))
    }

    fn agent_find(&self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let re = search::compile(&agent_search(args, String::new())?)?;
        let all = self.with_doc(self.tabs[i].doc, |s| sci::get_text(s));
        let hits = search::all_matches(&all, &re);
        let rows: Vec<Value> = hits
            .iter()
            .take(MAX_HITS)
            .map(|(start, end)| {
                let line = text::lines(&all, text::line_of(&all, *start), Some(1), MAX_HIT_LINE);
                json!({ "line": line.first, "start": start, "end": end, "text": line.text.trim_end() })
            })
            .collect();
        Ok(json!({ "tab": self.tabs[i].id, "count": hits.len(), "matches": rows, "more": hits.len() > MAX_HITS }))
    }

    fn agent_open(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let asked = PathBuf::from(args.text("path")?);
        let profile = crate::session::config_dir();
        let path = self.agent_reach(&profile).check(&asked)?;
        let size = std::fs::metadata(&path).map_err(|e| Error::Agent(format!("Cannot open {}: {e}", path.display())))?;
        if !size.is_file() {
            return Err(Error::Agent(format!("{} is not a file.", path.display())));
        }
        if size.len() > MAX_OPEN {
            return Err(Error::Agent(format!("{} is too large for an agent to open.", path.display())));
        }
        let ui = Headless::default();
        self.open_path(&ui, &path);
        match self.tabs.iter().find(|t| t.path.as_deref().is_some_and(|p| same_path(p, &path))) {
            Some(t) => Ok(json!({ "tab": t.id, "title": t.title, "kind": kind_name(t.kind) })),
            None => Err(ui.failure("The file was not opened. It may be encrypted, and agents cannot use encrypted documents.")),
        }
    }

    fn agent_switch(&mut self, args: Args<'_>) -> Result<Value, Error> {
        if args.number("tab")?.is_none() {
            return Err(Error::Agent("\"tab\" is required.".into()));
        }
        let i = self.agent_tab(args)?;
        self.activate(i);
        Ok(json!({ "tab": self.tabs[i].id, "title": self.tabs[i].title }))
    }
}

/// The search options of a find or a replace.
pub(super) fn agent_search(args: Args<'_>, replace: String) -> Result<search::Opts, Error> {
    Ok(search::Opts {
        find: args.text("find")?.to_string(),
        replace,
        match_case: args.flag("match_case")?,
        whole_word: args.flag("whole_word")?,
        regex: args.flag("regex")?,
        ..Default::default()
    })
}
