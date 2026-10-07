//! The server for one tenant: an actor that owns the tenant and takes updates in order, a worker
//! thread that schedules, and the HTTP and WebSocket front.

pub mod actor;
pub mod http;
mod worker;
