//! Who is connected. HTTP remembers nothing, so an agent gets a session when it starts and shows it with every call: that is
//! what "connected", approval and Stop are about.
//!
//! An agent is known by the name it gives. It can give any name, so approval is a check against surprises, not proof of who
//! is calling: the token is the lock on the door. Every session of a name gets the same answer.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::access::random_hex;
use crate::error::Error;

/// Sessions kept. An agent that never ends its session must not make this grow: the oldest goes.
const MAX_SESSIONS: usize = 32;
/// The most of a name that is shown.
const MAX_NAME: usize = 60;

/// What the user has said about an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Waiting,
    Allowed,
    Refused,
}

#[derive(Debug)]
struct Session {
    id: String,
    client: String,
    last_seen: Instant,
}

#[derive(Debug, Default)]
struct Book {
    sessions: Vec<Session>,
    trusted: Vec<String>,
    refused: Vec<String>,
    asking: Option<String>,
}

impl Book {
    fn standing_of(&self, client: &str) -> Standing {
        if self.trusted.iter().any(|c| c == client) {
            Standing::Allowed
        } else if self.refused.iter().any(|c| c == client) {
            Standing::Refused
        } else {
            Standing::Waiting
        }
    }
}

/// A name fit to show: one line of printable characters, not too long.
pub fn clean_name(name: &str) -> String {
    let shown: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    if shown.trim().is_empty() { "an unnamed agent".to_string() } else { shown.trim().to_string() }
}

/// The sessions, shared between the server and the window.
#[derive(Clone, Debug, Default)]
pub struct Sessions(Arc<Mutex<Book>>);

impl Sessions {
    fn book(&self) -> MutexGuard<'_, Book> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Start a session for the agent called `client`, and give its id.
    ///
    /// # Errors
    /// No secure random numbers.
    pub fn open(&self, client: &str) -> Result<String, Error> {
        let id = random_hex::<16>()?;
        let mut book = self.book();
        if book.sessions.len() >= MAX_SESSIONS
            && let Some(oldest) = book.sessions.iter().enumerate().min_by_key(|(_, s)| s.last_seen).map(|(i, _)| i)
        {
            book.sessions.remove(oldest);
        }
        book.sessions.push(Session { id: id.clone(), client: clean_name(client), last_seen: Instant::now() });
        Ok(id)
    }

    pub fn end(&self, id: &str) {
        self.book().sessions.retain(|s| s.id != id);
    }

    /// The agent of session `id` and what the user has said about it, noting that it was heard from. None for a session
    /// that does not exist: the agent has to start again.
    pub fn standing(&self, id: &str) -> Option<(String, Standing)> {
        let mut book = self.book();
        let session = book.sessions.iter_mut().find(|s| s.id == id)?;
        session.last_seen = Instant::now();
        let client = session.client.clone();
        let standing = book.standing_of(&client);
        Some((client, standing))
    }

    /// The agents the user has allowed for good.
    pub fn set_trusted(&self, names: &[String]) {
        self.book().trusted = names.to_vec();
    }

    /// An agent to ask the user about, one at a time.
    pub fn next_to_ask(&self) -> Option<String> {
        let mut book = self.book();
        if book.asking.is_some() {
            return None;
        }
        let client = book.sessions.iter().map(|s| s.client.clone()).find(|c| book.standing_of(c) == Standing::Waiting)?;
        book.asking = Some(client.clone());
        Some(client)
    }

    /// The user's answer. A refusal lasts until the server stops; what the user allows the window keeps in the settings.
    pub fn answer(&self, client: &str, allow: bool) {
        let mut book = self.book();
        book.asking = None;
        let list = if allow { &mut book.trusted } else { &mut book.refused };
        list.push(client.to_string());
    }

    /// The allowed agents heard from within `within`, each once.
    pub fn active(&self, within: Duration) -> Vec<String> {
        let book = self.book();
        let mut names: Vec<String> = book
            .sessions
            .iter()
            .filter(|s| s.last_seen.elapsed() <= within && book.standing_of(&s.client) == Standing::Allowed)
            .map(|s| s.client.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Forget every session and refusal: the server has stopped.
    pub fn clear(&self) {
        let mut book = self.book();
        book.sessions.clear();
        book.refused.clear();
        book.asking = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: Duration = Duration::from_secs(60);

    #[test]
    fn a_new_agent_waits_and_the_answer_goes_for_all_its_sessions() {
        let s = Sessions::default();
        let (a, b) = (s.open("claude").unwrap(), s.open("claude").unwrap());
        assert_ne!(a, b);
        assert_eq!(s.standing(&a), Some(("claude".into(), Standing::Waiting)));
        assert_eq!(s.next_to_ask().as_deref(), Some("claude"));
        s.answer("claude", true);
        assert_eq!(s.standing(&a).map(|x| x.1), Some(Standing::Allowed));
        assert_eq!(s.standing(&b).map(|x| x.1), Some(Standing::Allowed));
    }

    #[test]
    fn the_user_is_asked_one_at_a_time_and_only_once() {
        let s = Sessions::default();
        s.open("one").unwrap();
        s.open("two").unwrap();
        assert_eq!(s.next_to_ask().as_deref(), Some("one"));
        assert_eq!(s.next_to_ask(), None, "still waiting for the answer");
        s.answer("one", false);
        assert_eq!(s.next_to_ask().as_deref(), Some("two"));
        s.answer("two", true);
        assert_eq!(s.next_to_ask(), None);
    }

    #[test]
    fn a_trusted_agent_is_not_asked_about() {
        let s = Sessions::default();
        s.set_trusted(&["claude".to_string()]);
        let id = s.open("claude").unwrap();
        assert_eq!(s.next_to_ask(), None);
        assert_eq!(s.standing(&id).map(|x| x.1), Some(Standing::Allowed));
    }

    #[test]
    fn a_session_that_ended_is_not_known() {
        let s = Sessions::default();
        let id = s.open("x").unwrap();
        s.end(&id);
        assert_eq!(s.standing(&id), None);
        assert_eq!(s.standing("made up"), None);
    }

    #[test]
    fn only_allowed_agents_heard_from_lately_count_as_connected() {
        let s = Sessions::default();
        s.set_trusted(&["b".to_string(), "a".to_string()]);
        for name in ["b", "a", "b", "stranger"] {
            s.open(name).unwrap();
        }
        assert_eq!(s.active(LONG), ["a", "b"]);
        assert!(s.active(Duration::ZERO).is_empty());
    }

    #[test]
    fn stopping_forgets_sessions_and_refusals_but_not_who_is_trusted() {
        let s = Sessions::default();
        s.set_trusted(&["good".to_string()]);
        let bad = s.open("bad").unwrap();
        s.next_to_ask();
        s.answer("bad", false);
        s.clear();
        assert_eq!(s.standing(&bad), None);
        let again = s.open("bad").unwrap();
        assert_eq!(s.standing(&again).map(|x| x.1), Some(Standing::Waiting));
        let good = s.open("good").unwrap();
        assert_eq!(s.standing(&good).map(|x| x.1), Some(Standing::Allowed));
    }

    #[test]
    fn sessions_do_not_pile_up() {
        let s = Sessions::default();
        let first = s.open("x").unwrap();
        for _ in 0..MAX_SESSIONS {
            s.open("x").unwrap();
        }
        assert_eq!(s.standing(&first), None, "the oldest went");
    }

    #[test]
    fn a_name_is_made_fit_to_show() {
        assert_eq!(clean_name("  claude-code "), "claude-code");
        assert_eq!(clean_name("line\nbreak\u{7}"), "linebreak");
        assert_eq!(clean_name(""), "an unnamed agent");
        assert_eq!(clean_name("\n"), "an unnamed agent");
        assert_eq!(clean_name(&"x".repeat(500)).chars().count(), 60);
    }
}
