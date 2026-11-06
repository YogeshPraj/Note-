//! What keeps the user in charge of agents: the questions they are asked, the status bar's "AI connected", Stop, and the
//! activity log.

use notepp_core::commands::{ID_MCP_LOG, ID_MCP_STOP};
use notepp_core::mcp::access::Level;
use notepp_core::mcp::activity;

use super::*;
use crate::command::CommandHandler;

/// How long since an agent was last heard from that it still counts as connected. Agents can think for minutes.
const CONNECTED: std::time::Duration = std::time::Duration::from_secs(300);

/// Something only the user can decide.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Question {
    /// An agent nobody has answered for yet.
    Agent(String),
    /// An agent wants an encrypted tab: its id and title.
    Encrypted(u64, String),
}

fn what_it_may_do(level: Level) -> &'static str {
    match level {
        Level::Read => "It can see your open tabs and read them, but not change them.",
        Level::Edit => "It can read, change and save your open documents, whiteboards and pictures.",
        Level::Full => "It can read, change and save your open documents, whiteboards and pictures, and change the window.",
    }
}

impl Question {
    fn text(&self, level: Level) -> String {
        match self {
            Question::Agent(name) => format!(
                "An AI agent that calls itself \"{name}\" wants to work in Note++.\n\n{}\n\nOnly say Yes if you just connected this agent yourself.",
                what_it_may_do(level)
            ),
            Question::Encrypted(_, title) => format!(
                "An AI agent wants to use the encrypted document \"{title}\".\n\nIt would see the document's text. Allow it, until the agents are stopped?"
            ),
        }
    }
}

/// Add a tool call to the activity log.
pub(super) fn log_call(client: &str, tool: &str, target: &str, failure: Option<&str>) {
    activity::append(&activity::line(&notepp_core::crypto::iso_now(), client, tool, target, failure));
}

/// Add a decision to the activity log.
fn log_note(who: &str, what: &str, target: &str) {
    activity::append(&activity::note(&notepp_core::crypto::iso_now(), who, what, target));
}

/// Ask with nothing borrowed, so that the window and agents already allowed go on working meanwhile.
pub(super) fn ask(app: &Shared, question: &Question) {
    let Some((frame, text)) = app.try_borrow().ok().map(|a| (a.frame, question.text(Level::from_key(&a.ws.settings.mcp_level)))) else { return };
    let allowed = crate::widgets::ask::yes_no(&frame, "AI agent", &text, true);
    if let Ok(mut a) = app.try_borrow_mut() {
        a.mcp_answered(question, allowed);
    }
}

impl App {
    /// What the user has to be asked now, one question at a time.
    pub(super) fn mcp_question(&mut self) -> Option<Question> {
        if self.mcp.asking {
            return None;
        }
        let question = match self.ws.agent_asking() {
            Some((tab, title)) => Question::Encrypted(tab, title),
            None => Question::Agent(self.mcp.sessions.next_to_ask()?),
        };
        self.mcp.asking = true;
        Some(question)
    }

    fn mcp_answered(&mut self, question: &Question, allowed: bool) {
        self.mcp.asking = false;
        match question {
            Question::Agent(name) => {
                self.mcp.sessions.answer(name, allowed);
                if allowed {
                    let mut s = self.ws.settings.clone();
                    s.mcp_clients.push(name.clone());
                    crate::settings_ui::apply_settings(self, s);
                }
                log_note(name, if allowed { "approved by the user" } else { "refused by the user" }, "");
            }
            Question::Encrypted(tab, title) => {
                self.ws.agent_answer(*tab, allowed);
                log_note("the user", if allowed { "allowed agents to use" } else { "refused agents" }, title);
            }
        }
        self.refresh();
    }

    /// Keep the status bar's agents current. True when they changed.
    pub(super) fn mcp_watch(&mut self) -> bool {
        let active = self.mcp.sessions.active(CONNECTED);
        let changed = active != self.mcp.active;
        self.mcp.active = active;
        changed
    }

    /// What the status bar says about agents, when any is connected.
    pub(crate) fn mcp_label(&self) -> Option<String> {
        let first = self.mcp.active.first()?;
        Some(match self.mcp.active.len() {
            1 => format!("AI connected: {first}"),
            n => format!("AI connected: {first} +{}", n - 1),
        })
    }

    /// The agents the user has allowed, as the Settings page says it.
    pub(crate) fn mcp_trusted(&self) -> String {
        let names = &self.ws.settings.mcp_clients;
        if names.is_empty() { "No agent has been allowed yet.".to_string() } else { format!("Allowed: {}.", names.join(", ")) }
    }

    /// Stop everything agents are doing: the server goes off, and stays off until the user turns it on.
    fn mcp_stop(&mut self) {
        if !self.ws.settings.mcp_enabled {
            self.flash = Some("AI agents are off".into());
            return;
        }
        let mut s = self.ws.settings.clone();
        s.mcp_enabled = false;
        crate::settings_ui::apply_settings(self, s);
        log_note("the user", "stopped all agents", "");
        self.flash = Some("AI agents stopped. Turn them on again under Settings > AI Agents.".into());
    }

    /// Show the activity log in a tab of its own, as it is now.
    pub(crate) fn mcp_show_log(&mut self) {
        let text = activity::read();
        if text.is_empty() {
            self.flash = Some("No AI agent has done anything yet".into());
            return;
        }
        self.ws.show_snapshot("AI activity", &text);
    }

    /// Forget which agents were allowed: they will be asked about again. Agents working now stop at their next call.
    pub(crate) fn mcp_forget_trusted(&mut self) {
        let mut s = self.ws.settings.clone();
        s.mcp_clients.clear();
        crate::settings_ui::apply_settings(self, s);
        self.flash = Some("Agents will be asked about again".into());
    }
}

/// Stop and the activity log.
pub(crate) struct McpCommands;

impl CommandHandler<App> for McpCommands {
    fn name(&self) -> &'static str {
        "agents"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            ID_MCP_STOP => app.mcp_stop(),
            ID_MCP_LOG => app.mcp_show_log(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_says_who_asks_and_what_a_yes_would_mean() {
        let agent = Question::Agent("claude".into()).text(Level::Read);
        assert!(agent.contains("claude") && agent.contains("not change"));
        assert!(Question::Agent("x".into()).text(Level::Edit).contains("save"));
        assert!(Question::Encrypted(1, "diary.txt".into()).text(Level::Read).contains("diary.txt"));
    }
}
