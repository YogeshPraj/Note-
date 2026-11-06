//! The AI Agents page: the MCP server's switch, how much agents may do, and how to connect one.

use notepp_core::mcp::DEFAULT_PORT;
use notepp_core::mcp::access::Level;
use notepp_core::settings::Settings;

use super::{Env, button, choice, number, toggle};
use crate::widgets::settings_window::{Control, Page, Row, Value};

pub(super) fn page(env: &Env) -> Page {
    let s = &env.settings;
    let levels: Vec<(&str, &str)> = Level::ALL.iter().map(|l| (l.key(), l.label())).collect();
    Page::new(
        "agents",
        "AI Agents",
        0,
        vec![
            Row::new("", "AI agents", &env.mcp_status, Control::Heading),
            toggle(
                "mcp_on",
                "Let AI agents connect",
                "Run an MCP server, so an agent can work in Note++ without sending keystrokes. It listens on this computer only.",
                s.mcp_enabled,
            ),
            choice(
                "mcp_level",
                "What agents may do",
                "Read only: list, read, search and open. Edit: also change and save documents, each change one undo step. Encrypted documents and passwords are never available.",
                &levels,
                &s.mcp_level,
            ),
            number("mcp_port", "Port", "Change it if another program uses this port.", i64::from(s.mcp_port), 1024, 65535),
            button("mcp_copy", "Agent setup", "Copies the address and token, to paste into the agent's MCP settings.", "Copy agent setup"),
            button("mcp_token", "Token", "Make a new token if the old one may have been seen. Agents then need the new setup.", "New token"),
        ],
    )
}

/// Apply one of this page's values. False when the key is not one of them.
pub(super) fn apply_value(s: &mut Settings, key: &str, v: &Value) -> bool {
    match (key, v) {
        ("mcp_on", Value::Bool(b)) => s.mcp_enabled = *b,
        ("mcp_level", Value::Choice(i)) => s.mcp_level = Level::ALL.get(*i).copied().unwrap_or(Level::Read).key().to_string(),
        ("mcp_port", Value::Int(n)) => s.mcp_port = u16::try_from(*n).unwrap_or(DEFAULT_PORT),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_sets_what_the_server_reads() {
        let mut s = Settings::default();
        assert!(apply_value(&mut s, "mcp_on", &Value::Bool(true)));
        assert!(s.mcp_enabled);
        assert!(apply_value(&mut s, "mcp_level", &Value::Choice(1)));
        assert_eq!(Level::from_key(&s.mcp_level), Level::Edit);
        assert!(apply_value(&mut s, "mcp_level", &Value::Choice(9)));
        assert_eq!(Level::from_key(&s.mcp_level), Level::Read, "anything unknown is the lowest level");
        assert!(apply_value(&mut s, "mcp_port", &Value::Int(5000)));
        assert_eq!(s.mcp_port, 5000);
        assert!(apply_value(&mut s, "mcp_port", &Value::Int(99999)));
        assert_eq!(s.mcp_port, DEFAULT_PORT);
        assert!(!apply_value(&mut s, "other", &Value::Bool(true)));
    }
}
