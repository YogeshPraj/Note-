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

const LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_tabs"}}"#;

/// A server where the agent called `t` has been allowed.
fn server(queue: Queue) -> (Server, Sessions) {
    let sessions = Sessions::default();
    sessions.set_trusted(&["t".to_string()]);
    (Server::start(0, TOKEN, queue, sessions.clone()).unwrap(), sessions)
}

/// Start a session as `client`, and give the header that shows it.
fn join(port: u16, client: &str) -> String {
    let hello = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-11-25", "clientInfo": { "name": client } } });
    let reply = post(port, &bearer(), &hello.to_string());
    let id = reply.lines().find_map(|l| l.strip_prefix("Mcp-Session-Id: ")).unwrap_or_else(|| panic!("no session in {reply}"));
    format!("{}Mcp-Session-Id: {id}\r\n", bearer())
}

#[test]
fn a_tool_call_goes_through_the_queue_and_back() {
    let queue = Queue::default();
    let (server, _) = server(queue.clone());
    let (done, ui) = ui_thread(queue);
    let reply = post(server.port(), &join(server.port(), "t"), LIST);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains("ran list_tabs"), "{reply}");
    done.store(true, Ordering::SeqCst);
    ui.join().unwrap();
}

#[test]
fn more_callers_than_the_limit_all_get_their_answer() {
    let queue = Queue::default();
    let (server, _) = server(queue.clone());
    let (done, ui) = ui_thread(queue);
    let port = server.port();
    let headers = join(port, "t");
    let callers: Vec<_> = (0..MAX_CONNECTIONS * 3)
        .map(|_| {
            let headers = headers.clone();
            std::thread::spawn(move || post(port, &headers, LIST))
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
    let (server, _) = server(Queue::default());
    let port = server.port();
    assert!(post(port, "Authorization: Bearer wrong\r\n", "{}").starts_with("HTTP/1.1 401"));
    assert!(post(port, &format!("{}Origin: http://evil.example\r\n", bearer()), "{}").starts_with("HTTP/1.1 403"));
    assert!(post(port, &bearer(), "not json").starts_with("HTTP/1.1 400"));
    assert!(post(port, &bearer(), "{\"method\":\"notifications/initialized\"}").starts_with("HTTP/1.1 202"));
}

#[test]
fn stopping_frees_the_port_at_once() {
    let (server, _) = server(Queue::default());
    let port = server.port();
    drop(server);
    let again = Server::start(port, TOKEN, Queue::default(), Sessions::default()).unwrap();
    assert_eq!(again.port(), port);
}

#[test]
fn a_taken_port_is_an_error_not_a_panic() {
    let (first, _) = server(Queue::default());
    assert!(Server::start(first.port(), TOKEN, Queue::default(), Sessions::default()).is_err());
}

#[test]
fn stopping_tells_whoever_is_waiting() {
    let queue = Queue::default();
    let (server, _) = server(queue.clone());
    let waiter = queue.clone();
    let call = std::thread::spawn(move || waiter.call("t", "list_tabs", &json!({})));
    while queue.lock().is_empty() {
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(server);
    assert_eq!(call.join().unwrap(), Err(Error::Agent("The server was stopped.".into())));
}

#[test]
fn a_job_nobody_waits_for_any_more_says_so() {
    let (reply, _answer) = mpsc::channel();
    let job = Job { client: "t".into(), tool: "x".into(), args: json!({}), deadline: Instant::now(), reply };
    assert!(job.expired());
}

#[test]
fn a_tool_call_needs_a_session() {
    let (server, _) = server(Queue::default());
    assert!(post(server.port(), &bearer(), LIST).starts_with("HTTP/1.1 404"));
    let made_up = format!("{}Mcp-Session-Id: made-up\r\n", bearer());
    assert!(post(server.port(), &made_up, LIST).starts_with("HTTP/1.1 404"));
}

#[test]
fn a_call_waits_for_the_users_answer_and_then_runs() {
    let queue = Queue::default();
    let (server, sessions) = server(queue.clone());
    let (done, ui) = ui_thread(queue);
    let port = server.port();
    let headers = join(port, "stranger");
    let call = std::thread::spawn(move || post(port, &headers, LIST));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!call.is_finished(), "it waits");
    assert_eq!(sessions.next_to_ask().as_deref(), Some("stranger"));
    sessions.answer("stranger", true);
    assert!(call.join().unwrap().contains("ran list_tabs"));
    done.store(true, Ordering::SeqCst);
    ui.join().unwrap();
}

#[test]
fn a_refused_agent_runs_nothing_and_is_told() {
    let queue = Queue::default();
    let (server, sessions) = server(queue.clone());
    let headers = join(server.port(), "stranger");
    sessions.next_to_ask();
    sessions.answer("stranger", false);
    let reply = post(server.port(), &headers, LIST);
    assert!(reply.contains("refused this agent"), "{reply}");
    assert!(queue.pop().is_none(), "nothing reached the documents");
}

#[test]
fn a_session_ends_when_the_client_says_so() {
    let (server, _) = server(Queue::default());
    let port = server.port();
    let headers = join(port, "t");
    let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    write!(s, "DELETE /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}\r\n").unwrap();
    let mut reply = String::new();
    s.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(post(port, &headers, LIST).starts_with("HTTP/1.1 404"));
}

#[test]
fn a_session_starts_with_initialize_only() {
    let (server, _) = server(Queue::default());
    let ping = post(server.port(), &bearer(), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    assert!(ping.starts_with("HTTP/1.1 200") && !ping.contains("Mcp-Session-Id"));
}
