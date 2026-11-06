//! The agent tools that change something. Every change is one undo step, on a tab the agent can name without the user
//! leaving the one in front; all of it reaches the document through Scintilla.

use serde_json::{Value, json};

use super::agent::{Headless, agent_search};
use super::*;
use crate::error::Error;
use crate::mcp::args::Args;
use crate::mcp::text;

impl<S: Send> Workspace<S> {
    /// Carry out a tool of the Edit level.
    pub(super) fn agent_change(&mut self, tool: &str, args: Args<'_>) -> Result<Value, Error> {
        match tool {
            "new_document" => self.agent_new(args),
            "edit_text" => self.agent_replace_exact(args),
            "insert_text" => self.agent_insert(args),
            "write_document" => self.agent_write(args),
            "replace_all" => self.agent_replace_all(args),
            "save_document" => self.agent_save(args),
            "select_lines" => self.agent_select(args),
            // Commands are handled in the window: it calls `agent_command` and runs what that allows.
            "run_command" => Err(Error::Agent("Only the window can run commands.".into())),
            _ => Err(Error::Agent(format!("There is no tool \"{tool}\"."))),
        }
    }

    /// Run `f` on tab `i`'s document as one undo step, and mark the tab modified.
    fn agent_edit<R>(&mut self, i: usize, f: impl FnOnce(&S) -> Result<R, Error>) -> Result<R, Error> {
        if self.with_doc(self.tabs[i].doc, |s| s.send(sci::SCI_GETREADONLY, 0, 0)) != 0 {
            return Err(Error::Agent(format!("\"{}\" is read-only.", self.tabs[i].title)));
        }
        let result = self.with_doc(self.tabs[i].doc, |s| {
            s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
            let r = f(s);
            s.send(sci::SCI_ENDUNDOACTION, 0, 0);
            r
        });
        // A tab that is not in a view keeps its modified state here, and the hidden view tells nobody.
        self.tabs[i].dirty = true;
        result
    }

    fn agent_new(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.new_tab();
        if let Some(text) = args.opt_text("text")? {
            let body = text::with_eol(text, self.tabs[i].eol);
            self.agent_edit(i, |s| {
                search::replace_range(s, 0, 0, &body);
                Ok(())
            })?;
        }
        Ok(json!({ "tab": self.tabs[i].id, "title": self.tabs[i].title }))
    }

    fn agent_replace_exact(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let eol = self.tabs[i].eol;
        let all = self.with_doc(self.tabs[i].doc, |s| sci::get_text(s));
        let (old, _) = text::exact_match(&all, args.text("old_text")?, eol, args.flag("all")?)?;
        let options = search::Opts { find: old, replace: text::with_eol(args.text("new_text")?, eol), match_case: true, ..Default::default() };
        let replaced = self.agent_edit(i, |s| search::replace_all(s, &options))?;
        Ok(json!({ "tab": self.tabs[i].id, "replaced": replaced }))
    }

    fn agent_insert(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let body = text::with_eol(args.text("text")?, self.tabs[i].eol);
        let line = args.number("line")?;
        self.agent_edit(i, |s| {
            let end = sci::length(s);
            let at = line.filter(|l| *l > 0).map_or(end, |l| match s.send(sci::SCI_POSITIONFROMLINE, crate::num::to_usize(l - 1), 0) {
                p if p < 0 => end,
                p => p,
            });
            let at = crate::num::to_usize(at.max(0));
            search::replace_range(s, at, at, &body);
            Ok(())
        })?;
        Ok(json!({ "tab": self.tabs[i].id, "inserted": body.len() }))
    }

    fn agent_write(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let body = text::with_eol(args.text("text")?, self.tabs[i].eol);
        self.agent_edit(i, |s| {
            search::replace_range(s, 0, crate::num::to_usize(sci::length(s).max(0)), &body);
            Ok(())
        })?;
        Ok(json!({ "tab": self.tabs[i].id, "written": body.len() }))
    }

    fn agent_replace_all(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let options = agent_search(args, text::with_eol(args.text("replace")?, self.tabs[i].eol))?;
        if options.find.is_empty() {
            return Err(Error::Agent("\"find\" must not be empty.".into()));
        }
        let replaced = self.agent_edit(i, |s| search::replace_all(s, &options))?;
        Ok(json!({ "tab": self.tabs[i].id, "replaced": replaced }))
    }

    /// Where a save goes: the tab's own file, or a new file the agent names. Writing over another file would destroy it,
    /// and a file changed on disk since it was read would lose the other program's work, so both are refused.
    fn agent_save_target(&self, i: usize, args: Args<'_>) -> Result<Option<PathBuf>, Error> {
        let t = &self.tabs[i];
        let Some(asked) = args.opt_text("path")? else {
            let Some(path) = &t.path else {
                return Err(Error::Agent(format!("\"{}\" has no file yet: give a path in the workspace folder.", t.title)));
            };
            if t.disk.is_some() && stamp_of(path) != t.disk {
                return Err(Error::Agent(format!("{} changed on disk after it was read, so it was not saved. Ask the user.", path.display())));
            }
            return Ok(None);
        };
        let asked = PathBuf::from(asked);
        let profile = crate::session::config_dir();
        let target = self.agent_reach(&profile).check(&asked)?;
        if target.exists() && !t.path.as_deref().is_some_and(|own| same_path(own, &target)) {
            return Err(Error::Agent(format!("{} already exists, and agents do not overwrite other files.", target.display())));
        }
        Ok(Some(target))
    }

    fn agent_save(&mut self, args: Args<'_>) -> Result<Value, Error> {
        let i = self.agent_text_tab(args)?;
        let target = self.agent_save_target(i, args)?;
        let (id, keep) = (self.tabs[i].id, self.tabs[self.active].id);
        let ui = Headless::default();
        // Writing works on the active tab, as Save All does.
        self.activate(i);
        let saved = match target {
            Some(path) => self.write_and_adopt(&ui, path),
            None => self.save(&ui),
        };
        if let Some(back) = self.tabs.iter().position(|t| t.id == keep) {
            self.activate(back);
        }
        if !saved {
            return Err(ui.failure("It was not saved: the text has characters its encoding cannot hold."));
        }
        let t = self.tabs.iter().find(|t| t.id == id);
        Ok(json!({ "tab": id, "path": t.and_then(|t| t.path.as_ref()) }))
    }
}
