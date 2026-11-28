//! The server for one tenant: its actor, scheduling thread and HTTP/WebSocket front.

pub mod actor;
pub mod http;
mod worker;
