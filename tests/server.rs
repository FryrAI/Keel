// Server endpoint integration tests (Spec 010 - MCP + HTTP Server)
// Entry point that wires up all server test modules.

#[path = "server/test_http_server.rs"]
mod test_http_server;
#[path = "server/test_mcp_integration.rs"]
mod test_mcp_integration;
#[path = "server/test_mcp_server.rs"]
mod test_mcp_server;
#[path = "server/test_server_lifecycle.rs"]
mod test_server_lifecycle;
#[path = "server/test_watch_mode.rs"]
mod test_watch_mode;
