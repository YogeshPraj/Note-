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

/// What the user has said about encrypted tabs and agents, for as long as the agents run.
#[derive(Debug, Default)]
pub(super) struct Grants {
    allowed: Vec<u64>,
    refused: Vec<u64>,
    /// The tab to ask the user about.
    asking: std::cell::Cell<Option<u64>>,
}

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

pub(super) fn kind_name(kind: TabKind) -> &'static str {
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
            return Err(agent_commands::needs_more(tool, spec.level));
        }
        let args = Args(args);
        match tool {
            "list_tabs" => Ok(self.agent_list()),
            "read_document" => self.agent_read(args),
            "find_text" => self.agent_find(args),
            "brain_search" => self.agent_brain_search(args),
            "brain_overview" => self.agent_brain_overview(),
            "brain_read" => self.agent_brain_read(args),
            "open_file" => self.agent_open(args),
            "switch_tab" => self.agent_switch(args),
            "list_commands" => Ok(crate::mcp::commands::listing()),
            "whiteboard_guide" => Ok(Value::String(crate::mcp::page_tools::GUIDE.into())),
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
        if t.encrypted && !self.agent_grants.allowed.contains(&t.id) {
            if self.agent_grants.refused.contains(&t.id) {
                return Err(Error::Agent(format!("The user refused agents the encrypted document \"{}\". Do not ask again.", t.title)));
            }
            self.agent_grants.asking.set(Some(t.id));
            return Err(Error::Agent(format!(
                "\"{}\" is encrypted, so Note++ is asking the user whether agents may use it. Try again once they have answered.",
                t.title
            )));
        }
        Ok(i)
    }

    /// The encrypted tab an agent is waiting for, to ask the user about.
    pub fn agent_asking(&self) -> Option<(u64, String)> {
        let id = self.agent_grants.asking.take()?;
        self.tabs.iter().find(|t| t.id == id).map(|t| (id, t.title.clone()))
    }

    /// The user's answer about an encrypted tab.
    pub fn agent_answer(&mut self, tab: u64, allow: bool) {
        let list = if allow { &mut self.agent_grants.allowed } else { &mut self.agent_grants.refused };
        list.push(tab);
    }

    /// Forget those answers: the agents have stopped.
    pub fn agent_forget(&mut self) {
        self.agent_grants = Grants::default();
    }

    /// What a tool call is about, for the activity log: a tab's title or a path, never a document's text. Nothing for a
    /// tool that is not about a tab.
    pub fn agent_target(&self, tool: &str, args: &Value) -> String {
        let a = Args(args);
        if let Ok(Some(path)) = a.opt_text("path") {
            return path.to_string();
        }
        let about_a_tab = tools::find(tool).is_some_and(|t| (t.properties)().get("tab").is_some());
        if !about_a_tab || (tool == "preview_diagram" && args.get("text").is_some()) {
            return String::new();
        }
        match a.number("tab") {
            Ok(Some(_)) => self.agent_tab(a).map_or_else(|_| "a tab now closed".to_string(), |i| self.tabs[i].title.clone()),
            _ => self.tabs.get(self.active).map_or_else(String::new, |t| t.title.clone()),
        }
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
            None => Err(ui.failure("The file was not opened. If it is encrypted, the user has to open it.")),
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
