// Tests for MCP (Model Context Protocol) server implementation (Spec 010)
//
// Validates that keel exposes all required tools via MCP protocol,
// handles tool calls correctly, and returns properly formatted responses.
//
// use keel_server::mcp::{McpServer, McpToolRegistry, McpRequest, McpResponse};
// use keel_core::graph::GraphStore;

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_server_registers_all_tools() {
    // GIVEN a freshly initialized MCP server with a loaded graph
    // WHEN the server starts and advertises its capabilities
    // THEN all keel tools (compile, discover, map, explain, where, stats) are registered
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_compile_tool_returns_violations() {
    // GIVEN an MCP server with a mapped project containing a broken caller
    // WHEN the compile tool is invoked via MCP with a file containing the violation
    // THEN the response contains a CompileResult with the E001 broken_caller error
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_compile_tool_clean_returns_empty() {
    // GIVEN an MCP server with a mapped project that has no violations
    // WHEN the compile tool is invoked via MCP on a clean file
    // THEN the response contains an empty violations list and exit code 0
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_discover_tool_returns_adjacency() {
    // GIVEN an MCP server with a mapped project
    // WHEN the discover tool is invoked with a valid function hash
    // THEN the response contains callers, callees, and the node's metadata
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_discover_tool_unknown_hash() {
    // GIVEN an MCP server with a mapped project
    // WHEN the discover tool is invoked with a hash that doesn't exist in the graph
    // THEN the response contains an error indicating the hash was not found
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_map_tool_triggers_full_remap() {
    // GIVEN an MCP server with an existing graph
    // WHEN the map tool is invoked via MCP
    // THEN the graph is fully rebuilt and the response contains updated stats
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_explain_tool_returns_resolution_chain() {
    // GIVEN an MCP server with a mapped project containing a resolved call edge
    // WHEN the explain tool is invoked with the error code and hash
    // THEN the response includes the full resolution chain with tier info and confidence
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mcp_where_tool_resolves_hash_to_location() {
    // GIVEN an MCP server with a mapped project
    // WHEN the where tool is invoked with a valid function hash
    // THEN the response contains the file path and line number of the function
}
