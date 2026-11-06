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
use super::sessions::{Sessions, Standing};
use crate::error::Error;

/// How long a tool call waits for the UI thread. A dialog the user has open holds the app that long at most.
const WAIT: Duration = Duration::from_secs(15);
/// How long a call waits for the user to answer about an agent. Longer than is comfortable for a person to read a question,
/// and shorter than a client gives up.
const APPROVAL_WAIT: Duration = Duration::from_secs(30);
/// How long a client has to send its request, and to take the reply.
const IO_LIMIT: Duration = Duration::from_secs(10);
/// Connections served at once. More wait their turn: a local program must not be able to pile up threads.
const MAX_CONNECTIONS: usize = 16;

/// One tool call on its way to the UI thread.
#[derive(Debug)]
pub struct Job {
    /// The name of the agent calling.
    pub client: String,
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
    fn call(&self, client: &str, tool: &str, args: &Value) -> Result<Value, Error> {
        let (reply, answer) = mpsc::channel();
        self.lock().push_back(Job { client: client.to_string(), tool: tool.to_string(), args: args.clone(), deadline: Instant::now() + WAIT, reply });
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

/// The agent of session `id`, once the user lets it work. This waits on the connection's thread, so nothing else does.
fn approved(sessions: &Sessions, id: &str) -> Result<String, Error> {
    let until = Instant::now() + APPROVAL_WAIT;
    loop {
        match sessions.standing(id) {
            Some((client, Standing::Allowed)) => return Ok(client),
            Some((_, Standing::Refused)) => return Err(Error::Agent("The user refused this agent. Tell them, and do not try again.".into())),
            None => return Err(Error::Agent("The agents were stopped.".into())),
            Some((_, Standing::Waiting)) if Instant::now() >= until => {
                return Err(Error::Agent(
                    "The user has not answered yet: Note++ is asking them whether this agent may work. Ask them to look at Note++, then try again.".into(),
                ));
            }
            Some((_, Standing::Waiting)) => std::thread::sleep(Duration::from_millis(50)),
        }
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

/// What a connection needs from the server.
#[derive(Clone)]
struct Door {
    port: u16,
    token: Arc<str>,
    queue: Queue,
    sessions: Sessions,
}

/// Serve one request and close.
fn serve(mut stream: TcpStream, door: &Door) {
    if stream.set_read_timeout(Some(IO_LIMIT)).is_err() || stream.set_write_timeout(Some(IO_LIMIT)).is_err() {
        return;
    }
    let request = match http::read_request(&mut stream, IO_LIMIT) {
        Ok(Some(r)) => r,
        Ok(None) => return,
        Err(r) => return reject(&mut stream, r),
    };
    if let Err(r) = http::gate(&request, door.port, &door.token) {
        return reject(&mut stream, r);
    }
    let session = request.header("mcp-session-id").unwrap_or_default();
    if request.method == "DELETE" {
        door.sessions.end(session);
        return http::respond(&mut stream, 200, "{}");
    }
    let Ok(message) = serde_json::from_slice::<Value>(&request.body) else {
        let reply = protocol::error_reply(&Value::Null, protocol::PARSE_ERROR, "The body is not JSON");
        return http::respond(&mut stream, 400, &reply.to_string());
    };
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    // An unknown session tells a client to start again, which is how it comes back after Stop or a restart.
    if method == "tools/call" && door.sessions.standing(session).is_none() {
        return reject(&mut stream, Reject { status: 404, reason: "no such session" });
    }
    let run = |tool: &str, args: &Value| door.queue.call(&approved(&door.sessions, session)?, tool, args);
    match protocol::answer(&message, &run) {
        Some(reply) if method == "initialize" => match door.sessions.open(protocol::client_name(&message)) {
            Ok(id) => http::respond_with(&mut stream, 200, &reply.to_string(), &format!("Mcp-Session-Id: {id}\r\n")),
            Err(_) => reject(&mut stream, Reject { status: 503, reason: "no session" }),
        },
        Some(reply) => http::respond(&mut stream, 200, &reply.to_string()),
        None => http::respond(&mut stream, 202, ""),
    }
}

/// Accept one connection at a time as slots free up. Those beyond the limit wait in the system's queue: turning one away
/// unread would reset the connection, which a client sees as a failure instead of a short wait.
fn accept_loop(listener: &TcpListener, door: &Door, stop: &AtomicBool) {
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
        let door = door.clone();
        // A connection that gets no thread is simply dropped, and the slot with it.
        let _ = std::thread::Builder::new().name("mcp-connection".into()).spawn(move || {
            let _slot = slot;
            serve(stream, &door);
        });
    }
}

/// The running server. It stops when dropped.
#[derive(Debug)]
pub struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    queue: Queue,
    sessions: Sessions,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Listen on 127.0.0.1 only. Port 0 takes any free port.
    ///
    /// # Errors
    /// The port cannot be had, usually because another program, or another Note++, has it.
    pub fn start(port: u16, token: &str, queue: Queue, sessions: Sessions) -> Result<Server, Error> {
        let in_use = |e: std::io::Error| Error::Agent(format!("Port {port} cannot be used: {e}"));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(in_use)?;
        let port = listener.local_addr().map_err(in_use)?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let door = Door { port, token: token.into(), queue: queue.clone(), sessions: sessions.clone() };
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("mcp-listener".into())
            .spawn(move || accept_loop(&listener, &door, &flag))
            .map_err(|e| Error::Agent(format!("The server could not start: {e}")))?;
        Ok(Server { port, stop, queue, sessions, thread: Some(thread) })
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
        self.sessions.clear();
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
mod tests;
