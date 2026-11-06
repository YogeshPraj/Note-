//! Agent calls that a whiteboard's page carries out. A call waits here until the page holding its tab is ready, goes to the
//! page, and is answered when the page replies. Nothing waits on the UI thread meanwhile.

use notepp_core::error::Error;
use notepp_core::mcp::access::Level;
use notepp_core::mcp::server::Job;
use notepp_core::workspace::BoardRequest;

use super::*;

/// A tool call the page has not answered yet.
pub(super) struct Parked {
    id: u64,
    level: Level,
    request: BoardRequest,
    job: Job,
    sent: bool,
}

impl App {
    /// Take on a call for a page.
    pub(super) fn mcp_park(&mut self, level: Level, job: Job, request: BoardRequest) {
        self.mcp.next_op += 1;
        let id = self.mcp.next_op;
        self.mcp.parked.push(Parked { id, level, request, job, sent: false });
    }

    /// The page holding tab `tab`, once it has the tab's scene.
    fn board_page(&self, tab: u64) -> Option<WebPane> {
        if let Some((page, _)) = self.page_of(tab) {
            return page.is_ready().then_some(page);
        }
        let main = self.ws.main_is_whiteboard() && self.ws.tabs[self.ws.main_index()].id == tab;
        (main && self.wb.shown == Some(tab) && self.wbv.is_ready()).then(|| self.wbv.clone())
    }

    /// Send what is waiting for a page, and let go of calls nobody is waiting for any more.
    pub(super) fn mcp_pump(&mut self) {
        let parked = std::mem::take(&mut self.mcp.parked);
        for mut call in parked {
            if call.job.expired() {
                call.job.finish(Err(Error::Agent("The whiteboard did not answer in time.".into())));
                continue;
            }
            if !call.sent
                && let Some(page) = self.board_page(call.request.tab)
            {
                // From the main loop: running a script waits in a message loop of its own, and a quick answer would arrive
                // while the app is borrowed here, and be lost.
                let (id, op) = (call.id, call.request.op.clone());
                crate::widgets::defer::later(&self.frame, move || wbm::agent(&page, id, &op));
                call.sent = true;
            }
            self.mcp.parked.push(call);
        }
    }

    /// A page has answered.
    pub(crate) fn mcp_board_done(&mut self, done: wbm::AgentDone) {
        let Some(at) = self.mcp.parked.iter().position(|c| c.id == done.id) else { return };
        let call = self.mcp.parked.remove(at);
        if let Some(scene) = done.scene {
            self.scene_changed(call.request.tab, scene);
        }
        let result = match done.error {
            Some(message) => Err(Error::Agent(message)),
            None if call.request.save_after => self.ws.agent_call(call.level, "save_document", &call.job.args),
            None => Ok(done.result),
        };
        call.job.finish(result);
    }
}
