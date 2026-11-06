//! The listener and the way from a connection's thread to the UI thread.
//!
//! One thread accepts, and each connection gets a short-lived thread of its own. A tool call goes on the [`Queue`] and the
//! connection's thread waits; the shell takes it off on the UI thread, where the documents are.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::http::{self, Reject};
use super::protocol;
use crate::error::Error;

/// How long a tool call waits for the UI thread. A dialog the user has open holds the app that long at most.
const WAIT: Duration = Duration::from_secs(15);
/// How long a client has to send its request, and to take the reply.
const IO_LIMIT: Duration = Duration::from_secs(10);
/// Connections served at once. More wait their turn: a local program must not be able to pile up threads.
const MAX_CONNECTIONS: usize = 16;

/// One tool call on its way to the UI thread.
#[derive(Debug)]
pub struct Job {
    pub tool: String,
    pub args: Value,
    deadline: Instant,
    reply: Sender<Result<Value, Error>>,
}

impl Job {
    /// The caller has stopped waiting: this must not run, or an edit it was told failed would happen after all.
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Give the caller the result.
    pub fn finish(self, result: Result<Value, Error>) {
        let _ = self.reply.send(result);
    }
}

/// Tool calls waiting for the UI thread.
#[derive(Clone, Debug, Default)]
pub struct Queue(Arc<Mutex<VecDeque<Job>>>);

impl Queue {
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<Job>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The next call to run.
    pub fn pop(&self) -> Option<Job> {
        self.lock().pop_front()
    }

    /// Put a call on the queue and wait for its result.
    fn call(&self, tool: &str, args: &Value) -> Result<Value, Error> {
        let (reply, answer) = mpsc::channel();
        self.lock().push_back(Job { tool: tool.to_string(), args: args.clone(), deadline: Instant::now() + WAIT, reply });
        match answer.recv_timeout(WAIT) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                Err(Error::Agent("Note++ did not answer in time, so nothing was done. A dialog may be open: ask the user to close it.".into()))
            }
            Err(RecvTimeoutError::Disconnected) => Err(Error::Agent("The server was stopped.".into())),
        }
    }

    /// Answer nothing more: whoever is waiting is told the server stopped.
    fn clear(&self) {
        self.lock().clear();
    }
}

/// Counts a connection while its thread lives.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn reject(stream: &mut TcpStream, r: Reject) {
    http::respond(stream, r.status, &format!("{{\"error\":\"{}\"}}", r.reason));
}

/// Serve one request and close.
fn serve(mut stream: TcpStream, port: u16, token: &str, queue: &Queue) {
    if stream.set_read_timeout(Some(IO_LIMIT)).is_err() || stream.set_write_timeout(Some(IO_LIMIT)).is_err() {
        return;
    }
    let request = match http::read_request(&mut stream, IO_LIMIT) {
        Ok(Some(r)) => r,
        Ok(None) => return,
        Err(r) => return reject(&mut stream, r),
    };
    if let Err(r) = http::gate(&request, port, token) {
        return reject(&mut stream, r);
    }
    let Ok(message) = serde_json::from_slice::<Value>(&request.body) else {
        let reply = protocol::error_reply(&Value::Null, protocol::PARSE_ERROR, "The body is not JSON");
        return http::respond(&mut stream, 400, &reply.to_string());
    };
    match protocol::answer(&message, &|tool, args| queue.call(tool, args)) {
        Some(reply) => http::respond(&mut stream, 200, &reply.to_string()),
        None => http::respond(&mut stream, 202, ""),
    }
}

/// Accept one connection at a time as slots free up. Those beyond the limit wait in the system's queue: turning one away
/// unread would reset the connection, which a client sees as a failure instead of a short wait.
fn accept_loop(listener: &TcpListener, port: u16, token: &Arc<str>, queue: &Queue, stop: &AtomicBool) {
    let open = Arc::new(AtomicUsize::new(0));
    loop {
        while open.load(Ordering::SeqCst) >= MAX_CONNECTIONS && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        let accepted = listener.accept();
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok((stream, _)) = accepted else { continue };
        open.fetch_add(1, Ordering::SeqCst);
        let slot = Slot(open.clone());
        let (token, queue) = (token.clone(), queue.clone());
        // A connection that gets no thread is simply dropped, and the slot with it.
        let _ = std::thread::Builder::new().name("mcp-connection".into()).spawn(move || {
            let _slot = slot;
            serve(stream, port, &token, &queue);
        });
    }
}

/// The running server. It stops when dropped.
#[derive(Debug)]
pub struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    queue: Queue,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Listen on 127.0.0.1 only. Port 0 takes any free port.
    ///
    /// # Errors
    /// The port cannot be had, usually because another program, or another Note++, has it.
    pub fn start(port: u16, token: &str, queue: Queue) -> Result<Server, Error> {
        let in_use = |e: std::io::Error| Error::Agent(format!("Port {port} cannot be used: {e}"));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(in_use)?;
        let port = listener.local_addr().map_err(in_use)?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let (token, flag, jobs): (Arc<str>, _, _) = (token.into(), stop.clone(), queue.clone());
        let thread = std::thread::Builder::new()
            .name("mcp-listener".into())
            .spawn(move || accept_loop(&listener, port, &token, &jobs, &flag))
            .map_err(|e| Error::Agent(format!("The server could not start: {e}")))?;
        Ok(Server { port, stop, queue, thread: Some(thread) })
    }

    /// The port in use.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.queue.clear();
        // The listener sits in accept: a connection of our own wakes it to see the flag, and it can then be waited for, so
        // the port is free again when this returns. Without the connection it is left running rather than waited for.
        let here = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        if TcpStream::connect_timeout(&here, Duration::from_secs(1)).is_ok()
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    /// Stands in for the UI thread: answers every job until told to stop.
    fn ui_thread(queue: Queue) -> (Arc<AtomicBool>, JoinHandle<()>) {
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match queue.pop() {
                    Some(job) => {
                        let tool = job.tool.clone();
                        job.finish(Ok(json!(format!("ran {tool}"))));
                    }
                    None => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        });
        (done, thread)
    }

    fn post(port: u16, headers: &str, body: &str) -> String {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(s, "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        reply
    }

    fn bearer() -> String {
        format!("Authorization: Bearer {TOKEN}\r\n")
    }

    #[test]
    fn a_tool_call_goes_through_the_queue_and_back() {
        let queue = Queue::default();
        let server = Server::start(0, TOKEN, queue.clone()).unwrap();
        let (done, ui) = ui_thread(queue);
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_tabs" } }).to_string();
        let reply = post(server.port(), &bearer(), &body);
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert!(reply.contains("ran list_tabs"), "{reply}");
        done.store(true, Ordering::SeqCst);
        ui.join().unwrap();
    }

    #[test]
    fn more_callers_than_the_limit_all_get_their_answer() {
        let queue = Queue::default();
        let server = Server::start(0, TOKEN, queue.clone()).unwrap();
        let (done, ui) = ui_thread(queue);
        let port = server.port();
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_tabs" } }).to_string();
        let callers: Vec<_> = (0..MAX_CONNECTIONS * 3)
            .map(|_| {
                let body = body.clone();
                std::thread::spawn(move || post(port, &bearer(), &body))
            })
            .collect();
        for caller in callers {
            assert!(caller.join().unwrap().contains("ran list_tabs"));
        }
        done.store(true, Ordering::SeqCst);
        ui.join().unwrap();
    }

    #[test]
    fn the_door_holds_on_a_real_socket() {
        let server = Server::start(0, TOKEN, Queue::default()).unwrap();
        let port = server.port();
        assert!(post(port, "Authorization: Bearer wrong\r\n", "{}").starts_with("HTTP/1.1 401"));
        assert!(post(port, &format!("{}Origin: http://evil.example\r\n", bearer()), "{}").starts_with("HTTP/1.1 403"));
        assert!(post(port, &bearer(), "not json").starts_with("HTTP/1.1 400"));
        assert!(post(port, &bearer(), "{\"method\":\"notifications/initialized\"}").starts_with("HTTP/1.1 202"));
    }

    #[test]
    fn stopping_frees_the_port_at_once() {
        let server = Server::start(0, TOKEN, Queue::default()).unwrap();
        let port = server.port();
        drop(server);
        let again = Server::start(port, TOKEN, Queue::default()).unwrap();
        assert_eq!(again.port(), port);
    }

    #[test]
    fn a_taken_port_is_an_error_not_a_panic() {
        let first = Server::start(0, TOKEN, Queue::default()).unwrap();
        assert!(Server::start(first.port(), TOKEN, Queue::default()).is_err());
    }

    #[test]
    fn stopping_tells_whoever_is_waiting() {
        let queue = Queue::default();
        let server = Server::start(0, TOKEN, queue.clone()).unwrap();
        let waiter = queue.clone();
        let call = std::thread::spawn(move || waiter.call("list_tabs", &json!({})));
        while queue.lock().is_empty() {
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(server);
        assert_eq!(call.join().unwrap(), Err(Error::Agent("The server was stopped.".into())));
    }

    #[test]
    fn a_job_nobody_waits_for_any_more_says_so() {
        let (reply, _answer) = mpsc::channel();
        let job = Job { tool: "x".into(), args: json!({}), deadline: Instant::now(), reply };
        assert!(job.expired());
    }
}
