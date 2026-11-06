//! The MCP server in the window: starting and stopping it with the settings, and running its tool calls on the UI thread.

use notepp_core::error::Error;
use notepp_core::mcp::access::{self, Level};
use notepp_core::mcp::server::{Queue, Server};
use notepp_core::mcp::tools;

use super::*;

/// How often waiting tool calls are looked for, while the server is on. It costs nothing while it is off, and at this rate
/// a tool call waits 25 ms on average, which an agent never notices.
const TICK_MS: i32 = 50;

/// The running server and what links it to the window.
#[derive(Default)]
pub(crate) struct McpState {
    server: Option<Server>,
    queue: Queue,
    timer: Option<Timer<Frame>>,
    /// Why the server is not running although it should be.
    problem: Option<String>,
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
        if let Ok(mut app) = a.try_borrow_mut() {
            app.mcp_tick();
        }
    });
    if let Ok(mut app) = app.try_borrow_mut() {
        app.mcp.timer = Some(timer);
        app.mcp_sync();
    }
}

fn start(port: u16, queue: Queue) -> Result<Server, Error> {
    Server::start(port, &access::token()?, queue)
}

impl App {
    /// Make the server match the settings: on or off, on the chosen port.
    pub(crate) fn mcp_sync(&mut self) {
        let s = &self.ws.settings;
        let want = s.mcp_enabled.then_some(s.mcp_port);
        if self.mcp.server.as_ref().map(Server::port) == want {
            return;
        }
        // Dropping it frees the port first, and tells any waiting call that it stopped.
        self.mcp.server = None;
        self.mcp.parked.clear();
        self.mcp.problem = None;
        if let Some(port) = want {
            match start(port, self.mcp.queue.clone()) {
                Ok(server) => self.mcp.server = Some(server),
                Err(e) => {
                    self.flash = Some(format!("The MCP server could not start. {e}"));
                    self.mcp.problem = Some(e.to_string());
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
            match self.ws.agent_board(level, &job.tool, &job.args) {
                Ok(Some(request)) => {
                    self.mcp_park(level, job, request);
                    ran = true;
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    job.finish(Err(e));
                    continue;
                }
            }
            let result = if job.tool == "run_command" { self.mcp_command(level, &job.args) } else { self.ws.agent_call(level, &job.tool, &job.args) };
            job.finish(result);
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
            (Some(server), _) => format!("Running at http://127.0.0.1:{}/mcp. Only programs on this computer that have the token can connect.", server.port()),
            (None, Some(problem)) => format!("Not running. {problem}"),
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
