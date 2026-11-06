//! The MCP server: lets an AI agent work in the running Note++ instead of sending it keystrokes.
//!
//! Off by default. When on, a thread listens on 127.0.0.1 and speaks just enough HTTP and JSON-RPC (Streamable HTTP with
//! plain JSON replies, no event streams). Nothing here touches a document: a tool call becomes a [`server::Job`] on a
//! [`server::Queue`], the shell runs it on the UI thread through `Workspace::agent_call`, and the worker waits for the answer.

pub mod access;
pub mod args;
pub mod commands;
pub mod http;
pub mod protocol;
pub mod server;
pub mod text;
pub mod tools;

/// Where the server listens unless the user chooses another port.
pub const DEFAULT_PORT: u16 = 47811;
