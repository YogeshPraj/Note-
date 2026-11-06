//! The MCP server in the window: starting and stopping it with the settings, and running its tool calls on the UI thread.

use notepp_core::error::Error;
use notepp_core::mcp::access::{self, Level};
use notepp_core::mcp::server::{Queue, Server};
use notepp_core::mcp::sessions::Sessions;
use notepp_core::mcp::tools;

use super::*;

/// How often waiting tool calls are looked for, while the server is on. It costs nothing while it is off, and at this rate
/// a tool call waits 25 ms on average, which an agent never notices.
const TICK_MS: i32 = 50;

/// How many half-second ticks to wait before trying a busy port again.
const RETRY_TICKS: u32 = 6;

/// The running server and what links it to the window.
#[derive(Default)]
pub(crate) struct McpState {
    server: Option<Server>,
    queue: Queue,
    timer: Option<Timer<Frame>>,
    /// Why the server is not running although it should be.
    problem: Option<String>,
    /// Ticks until the port is tried again.
    retry_in: u32,
    pub(super) sessions: Sessions,
    /// A question is on the screen.
    pub(super) asking: bool,
    /// The agents the status bar shows.
    pub(super) active: Vec<String>,
    /// Calls that wait for a whiteboard's page.
    pub(super) parked: Vec<super::mcp_board::Parked>,
    /// The last request number given to a page.
    pub(super) next_op: u64,
}

/// The server's clock, then the server itself if the settings say so.
pub(super) fn wire_mcp(app: &Shared, frame: Frame) {
    let timer = Timer::new(&frame);
    let a = app.clone();
    timer.on_tick(move |_| {
        // While a dialog holds the app the calls wait, and fail after a while with a message that says why.
        let question = a.try_borrow_mut().ok().and_then(|mut app| {
            app.mcp_tick();
            app.mcp_question()
        });
        if let Some(question) = question {
            let asker = a.clone();
            crate::widgets::defer::later(&frame, move || super::mcp_safety::ask(&asker, &question));
        }
    });
    if let Ok(mut app) = app.try_borrow_mut() {
        app.mcp.timer = Some(timer);
        app.mcp_sync();
    }
}

fn start(port: u16, queue: Queue, sessions: Sessions) -> Result<Server, Error> {
    Server::start(port, &access::token()?, queue, sessions)
}

impl App {
    /// Make the server match the settings: on or off, on the chosen port.
    pub(crate) fn mcp_sync(&mut self) {
        let s = &self.ws.settings;
        let want = s.mcp_enabled.then_some(s.mcp_port);
        self.mcp.sessions.set_trusted(&s.mcp_clients);
        if self.mcp.server.as_ref().map(Server::port) == want {
            return;
        }
        // Dropping it frees the port first, and tells any waiting call that it stopped.
        let was = self.mcp.problem.take();
        self.mcp.server = None;
        self.mcp.parked.clear();
        self.mcp.asking = false;
        self.ws.agent_forget();
        if let Some(port) = want {
            match start(port, self.mcp.queue.clone(), self.mcp.sessions.clone()) {
                Ok(server) => {
                    self.mcp.server = Some(server);
                    if was.is_some() {
                        self.flash = Some("The MCP server is running now".into());
                    }
                }
                Err(e) => {
                    let problem = e.to_string();
                    // Said once: it is tried again every few seconds.
                    if was.as_ref() != Some(&problem) {
                        self.flash = Some(format!("The MCP server could not start, and will keep trying. {problem}"));
                    }
                    self.mcp.problem = Some(problem);
                }
            }
        }
        if let Some(timer) = &self.mcp.timer {
            if self.mcp.server.is_some() {
                timer.start(TICK_MS, false);
            } else {
                timer.stop();
            }
        }
        // The clock has stopped, and it is what notices agents going.
        self.mcp_watch();
    }

    /// Try again while the port is taken: whatever holds it may let go, and then agents must not stay locked out with
    /// the box still ticked.
    pub(crate) fn mcp_retry(&mut self) {
        if self.mcp.problem.is_none() {
            return;
        }
        self.mcp.retry_in = self.mcp.retry_in.saturating_sub(1);
        if self.mcp.retry_in == 0 {
            self.mcp.retry_in = RETRY_TICKS;
            self.mcp_sync();
            if self.mcp.server.is_some() {
                self.refresh();
            }
        }
    }

    /// Run the tool calls that are waiting.
    fn mcp_tick(&mut self) {
        let mut ran = false;
        while let Some(job) = self.mcp.queue.pop() {
            if job.expired() {
                continue;
            }
            let level = Level::from_key(&self.ws.settings.mcp_level);
            if let Some(tool) = tools::find(&job.tool).filter(|t| t.level > Level::Read) {
                self.flash = Some(format!("AI agent: {}", tool.title));
            }
            let target = self.ws.agent_target(&job.tool, &job.args);
            match self.ws.agent_board(level, &job.tool, &job.args) {
                Ok(Some(request)) => {
                    self.mcp_park(level, job, request, target);
                    ran = true;
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    super::mcp_safety::log_call(&job.client, &job.tool, &target, Some(&e.to_string()));
                    job.finish(Err(e));
                    continue;
                }
            }
            let result = if job.tool == "run_command" { self.mcp_command(level, &job.args) } else { self.ws.agent_call(level, &job.tool, &job.args) };
            super::mcp_safety::log_call(&job.client, &job.tool, &target, result.as_ref().err().map(ToString::to_string).as_deref());
            job.finish(result);
            ran = true;
        }
        if self.mcp_watch() {
            ran = true;
        }
        if ran {
            self.refresh();
        }
        self.mcp_pump();
    }

    /// Run a registry command an agent named, through the same path a menu takes, and pass on what it reported.
    fn mcp_command(&mut self, level: Level, args: &serde_json::Value) -> Result<serde_json::Value, Error> {
        let id = self.ws.agent_command(level, args)?;
        self.flash = None;
        self.ws.agent_undo_group(id, true);
        let ran = crate::command::execute(self, id);
        self.ws.agent_undo_group(id, false);
        let said = self.flash.take();
        let name = notepp_core::registry::spec(id).map_or("", |s| s.palette);
        self.flash = Some(format!("AI agent: {name}"));
        match ran {
            Ok(()) => Ok(serde_json::json!({ "command": name, "message": said })),
            Err(e) => Err(Error::Agent(e.to_string())),
        }
    }

    /// What the Settings page says about the server.
    pub(crate) fn mcp_status(&self) -> String {
        match (&self.mcp.server, &self.mcp.problem) {
            (Some(server), _) => format!(
                "Running at http://127.0.0.1:{}/mcp. Only programs on this computer that have the token can connect, and you are asked about each new agent.",
                server.port()
            ),
            (None, Some(problem)) => format!("Not running yet: it keeps trying. {problem}"),
            (None, None) => "Off. Agents cannot connect.".into(),
        }
    }

    /// Put the address and token on the clipboard, as a password is: out of the history, and gone soon after.
    pub(crate) fn mcp_copy_setup(&mut self) {
        self.flash = Some(match access::token() {
            Ok(token) => {
                if self.clipboard.copy_secret(&access::setup_text(self.ws.settings.mcp_port, &token)) {
                    format!("Agent setup copied. It leaves the clipboard in {} seconds.", notepp_core::secret::SECRET_TTL.as_secs())
                } else {
                    "The clipboard is busy. Try again.".into()
                }
            }
            Err(e) => e.to_string(),
        });
    }

    /// Replace the token. The server starts again with it, so agents using the old one are out at once.
    pub(crate) fn mcp_new_token(&mut self) {
        match access::new_token() {
            Ok(_) => {
                self.mcp.server = None;
                self.mcp_sync();
                self.flash = Some("New token made. Agents need the new setup.".into());
            }
            Err(e) => self.flash = Some(e.to_string()),
        }
    }
}
