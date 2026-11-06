//! Agent calls that a whiteboard's or picture's page carries out. A call waits here until the page holding its tab is ready, goes to the
//! page, and is answered when the page replies. Nothing waits on the UI thread meanwhile.

use notepp_core::error::Error;
use notepp_core::mcp::access::Level;
use notepp_core::mcp::server::Job;
use notepp_core::workspace::{BoardRequest, PREVIEW_PAGE, TabKind};

use super::*;

/// A tool call the page has not answered yet.
pub(super) struct Parked {
    id: u64,
    level: Level,
    request: BoardRequest,
    job: Job,
    /// What the activity log says it was about.
    target: String,
    sent: bool,
}

impl App {
    /// Take on a call for a page.
    pub(super) fn mcp_park(&mut self, level: Level, job: Job, request: BoardRequest, target: String) {
        self.mcp.next_op += 1;
        let id = self.mcp.next_op;
        self.mcp.parked.push(Parked { id, level, request, job, target, sent: false });
    }

    /// The page holding tab `tab`, once it has the tab's scene.
    fn board_page(&self, tab: u64) -> Option<WebPane> {
        if tab == PREVIEW_PAGE {
            if !self.view.is_loaded() {
                self.view.load_html(&preview::page_html(self.ws.settings.preview_remote_images));
            }
            return self.view.is_ready().then(|| self.view.clone());
        }
        if let Some((page, _)) = self.page_of(tab) {
            return page.is_ready().then_some(page);
        }
        let shown = self.ws.tabs.get(self.ws.main_index()).filter(|t| t.id == tab)?;
        match shown.kind {
            TabKind::Whiteboard => (self.wb.shown == Some(tab) && self.wbv.is_ready()).then(|| self.wbv.clone()),
            TabKind::Image => (self.img.shown == Some(tab) && self.imgv.is_ready()).then(|| self.imgv.clone()),
            TabKind::Text => None,
        }
    }

    /// Send what is waiting for a page, and let go of calls nobody is waiting for any more.
    pub(super) fn mcp_pump(&mut self) {
        let parked = std::mem::take(&mut self.mcp.parked);
        for mut call in parked {
            if call.job.expired() {
                call.job.finish(Err(Error::Agent("The page did not answer in time.".into())));
                continue;
            }
            if !call.sent
                && let Some(page) = self.board_page(call.request.tab)
            {
                // From the main loop: running a script waits in a message loop of its own, and a quick answer would arrive
                // while the app is borrowed here, and be lost.
                let (id, op) = (call.id, call.request.op.clone());
                let tab = call.request.tab;
                let picture = self.ws.tabs.iter().any(|t| t.id == tab && t.kind == TabKind::Image);
                crate::widgets::defer::later(&self.frame, move || {
                    if tab == PREVIEW_PAGE {
                        preview::agent(&page, id, &op);
                    } else if picture {
                        crate::imageedit::agent(&page, id, &op);
                    } else {
                        wbm::agent(&page, id, &op);
                    }
                });
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
            // Only the app knows the tab of a picture it has just made.
            None if call.job.tool == "new_picture" => Ok(serde_json::json!({ "tab": call.request.tab, "picture": done.result })),
            None => Ok(done.result),
        };
        super::mcp_safety::log_call(&call.job.client, &call.job.tool, &call.target, result.as_ref().err().map(ToString::to_string).as_deref());
        call.job.finish(result);
    }
}
