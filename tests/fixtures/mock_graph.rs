use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind,
};

/// Helper to build a GraphNode with sensible defaults.
#[allow(clippy::too_many_arguments)]
fn make_node(
    id: u64,
    hash: &str,
    kind: NodeKind,
    name: &str,
    file_path: &str,
    line_start: u32,
    line_end: u32,
    module_id: u64,
    is_public: bool,
) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        file_path: file_path.to_string(),
        line_start,
        line_end,
        docstring: None,
        is_public,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

/// Helper to build a GraphEdge.
fn make_edge(id: u64, source_id: u64, target_id: u64, kind: EdgeKind, file_path: &str, line: u32) -> GraphEdge {
    GraphEdge {
        id,
        source_id,
        target_id,
        kind,
        file_path: file_path.to_string(),
        line,
        confidence: 1.0,
    }
}

/// Create a pre-populated in-memory SqliteGraphStore with known test data.
///
/// Contains:
/// - 5 modules (ids 100-104)
/// - 20 functions (ids 1-20, 4 per module)
/// - 30 edges (calls, imports, contains)
///
/// Module layout:
///   module_auth     (100): authenticate, authorize, validate_token, refresh_token
///   module_users    (101): get_user, create_user, update_user, delete_user
///   module_api      (102): handle_request, parse_body, send_response, log_request
///   module_db       (103): connect, query, insert, close
///   module_utils    (104): hash_password, generate_id, format_date, parse_config
pub fn create_test_graph() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory()
        .expect("Failed to create in-memory SqliteGraphStore");

    // --- Modules ---
    let modules = vec![
        make_node(100, "mod_auth_hash", NodeKind::Module, "module_auth", "src/auth.rs", 1, 100, 0, true),
        make_node(101, "mod_users_hsh", NodeKind::Module, "module_users", "src/users.rs", 1, 120, 0, true),
        make_node(102, "mod_api_hash0", NodeKind::Module, "module_api", "src/api.rs", 1, 80, 0, true),
        make_node(103, "mod_db_hash00", NodeKind::Module, "module_db", "src/db.rs", 1, 60, 0, true),
        make_node(104, "mod_utils_hsh", NodeKind::Module, "module_utils", "src/utils.rs", 1, 90, 0, true),
    ];

    // --- Functions (4 per module, ids 1-20) ---
    let functions = vec![
        // auth module (100)
        make_node(1,  "fn_auth_00001", NodeKind::Function, "authenticate",   "src/auth.rs",  5,  20, 100, true),
        make_node(2,  "fn_auth_00002", NodeKind::Function, "authorize",      "src/auth.rs", 22,  40, 100, true),
        make_node(3,  "fn_auth_00003", NodeKind::Function, "validate_token", "src/auth.rs", 42,  60, 100, true),
        make_node(4,  "fn_auth_00004", NodeKind::Function, "refresh_token",  "src/auth.rs", 62,  80, 100, false),
        // users module (101)
        make_node(5,  "fn_user_00005", NodeKind::Function, "get_user",    "src/users.rs",  5,  25, 101, true),
        make_node(6,  "fn_user_00006", NodeKind::Function, "create_user", "src/users.rs", 27,  50, 101, true),
        make_node(7,  "fn_user_00007", NodeKind::Function, "update_user", "src/users.rs", 52,  75, 101, true),
        make_node(8,  "fn_user_00008", NodeKind::Function, "delete_user", "src/users.rs", 77, 100, 101, true),
        // api module (102)
        make_node(9,  "fn_api_00009",  NodeKind::Function, "handle_request", "src/api.rs",  5,  20, 102, true),
        make_node(10, "fn_api_00010",  NodeKind::Function, "parse_body",     "src/api.rs", 22,  35, 102, false),
        make_node(11, "fn_api_00011",  NodeKind::Function, "send_response",  "src/api.rs", 37,  55, 102, true),
        make_node(12, "fn_api_00012",  NodeKind::Function, "log_request",    "src/api.rs", 57,  70, 102, false),
        // db module (103)
        make_node(13, "fn_db_000013",  NodeKind::Function, "connect", "src/db.rs",  5,  15, 103, true),
        make_node(14, "fn_db_000014",  NodeKind::Function, "query",   "src/db.rs", 17,  30, 103, true),
        make_node(15, "fn_db_000015",  NodeKind::Function, "insert",  "src/db.rs", 32,  45, 103, true),
        make_node(16, "fn_db_000016",  NodeKind::Function, "close",   "src/db.rs", 47,  55, 103, true),
        // utils module (104)
        make_node(17, "fn_util_00017", NodeKind::Function, "hash_password", "src/utils.rs",  5,  20, 104, true),
        make_node(18, "fn_util_00018", NodeKind::Function, "generate_id",   "src/utils.rs", 22,  35, 104, true),
        make_node(19, "fn_util_00019", NodeKind::Function, "format_date",   "src/utils.rs", 37,  50, 104, true),
        make_node(20, "fn_util_00020", NodeKind::Function, "parse_config",  "src/utils.rs", 52,  70, 104, true),
    ];

    // Insert all nodes
    let mut all_nodes: Vec<NodeChange> = modules.into_iter().map(NodeChange::Add).collect();
    all_nodes.extend(functions.into_iter().map(NodeChange::Add));
    store.update_nodes(all_nodes).expect("Failed to insert nodes");

    // --- Edges (30 total) ---
    let edges = vec![
        // Contains edges: modules contain their functions
        make_edge(1,  100, 1,  EdgeKind::Contains, "src/auth.rs",  5),
        make_edge(2,  100, 2,  EdgeKind::Contains, "src/auth.rs", 22),
        make_edge(3,  100, 3,  EdgeKind::Contains, "src/auth.rs", 42),
        make_edge(4,  100, 4,  EdgeKind::Contains, "src/auth.rs", 62),
        make_edge(5,  101, 5,  EdgeKind::Contains, "src/users.rs",  5),
        make_edge(6,  101, 6,  EdgeKind::Contains, "src/users.rs", 27),
        make_edge(7,  101, 7,  EdgeKind::Contains, "src/users.rs", 52),
        make_edge(8,  101, 8,  EdgeKind::Contains, "src/users.rs", 77),
        make_edge(9,  102, 9,  EdgeKind::Contains, "src/api.rs",  5),
        make_edge(10, 102, 10, EdgeKind::Contains, "src/api.rs", 22),
        // Call edges: api -> auth, api -> users, users -> db, auth -> utils
        make_edge(11, 9,  1,  EdgeKind::Calls, "src/api.rs",  10),  // handle_request -> authenticate
        make_edge(12, 9,  2,  EdgeKind::Calls, "src/api.rs",  12),  // handle_request -> authorize
        make_edge(13, 9,  5,  EdgeKind::Calls, "src/api.rs",  14),  // handle_request -> get_user
        make_edge(14, 9,  10, EdgeKind::Calls, "src/api.rs",   8),  // handle_request -> parse_body
        make_edge(15, 9,  11, EdgeKind::Calls, "src/api.rs",  18),  // handle_request -> send_response
        make_edge(16, 9,  12, EdgeKind::Calls, "src/api.rs",   6),  // handle_request -> log_request
        make_edge(17, 6,  14, EdgeKind::Calls, "src/users.rs", 30), // create_user -> query
        make_edge(18, 6,  15, EdgeKind::Calls, "src/users.rs", 35), // create_user -> insert
        make_edge(19, 7,  14, EdgeKind::Calls, "src/users.rs", 55), // update_user -> query
        make_edge(20, 8,  14, EdgeKind::Calls, "src/users.rs", 80), // delete_user -> query
        make_edge(21, 5,  14, EdgeKind::Calls, "src/users.rs", 10), // get_user -> query
        make_edge(22, 1,  17, EdgeKind::Calls, "src/auth.rs",  10), // authenticate -> hash_password
        make_edge(23, 1,  3,  EdgeKind::Calls, "src/auth.rs",  15), // authenticate -> validate_token
        make_edge(24, 6,  18, EdgeKind::Calls, "src/users.rs", 32), // create_user -> generate_id
        // Import edges: modules importing other modules
        make_edge(25, 102, 100, EdgeKind::Imports, "src/api.rs",   1), // api imports auth
        make_edge(26, 102, 101, EdgeKind::Imports, "src/api.rs",   2), // api imports users
        make_edge(27, 101, 103, EdgeKind::Imports, "src/users.rs", 1), // users imports db
        make_edge(28, 100, 104, EdgeKind::Imports, "src/auth.rs",  1), // auth imports utils
        make_edge(29, 102, 103, EdgeKind::Imports, "src/api.rs",   3), // api imports db
        make_edge(30, 101, 104, EdgeKind::Imports, "src/users.rs", 2), // users imports utils
    ];

    let edge_changes: Vec<EdgeChange> = edges.into_iter().map(EdgeChange::Add).collect();
    store.update_edges(edge_changes).expect("Failed to insert edges");

    store
}

/// Create an empty in-memory SqliteGraphStore.
pub fn create_empty_graph() -> SqliteGraphStore {
    SqliteGraphStore::in_memory()
        .expect("Failed to create in-memory SqliteGraphStore")
}

/// Create a single-module graph with 3 functions and their edges.
///
/// Layout:
///   module_math (100): add, subtract, multiply
///   add -> subtract (calls), module_math contains all three
pub fn create_single_module_graph() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory()
        .expect("Failed to create in-memory SqliteGraphStore");

    let nodes = vec![
        NodeChange::Add(make_node(
            100, "mod_math_hash", NodeKind::Module, "module_math",
            "src/math.rs", 1, 50, 0, true,
        )),
        NodeChange::Add(GraphNode {
            id: 1,
            hash: "fn_add_hash00".to_string(),
            kind: NodeKind::Function,
            name: "add".to_string(),
            signature: "fn add(a: i32, b: i32) -> i32".to_string(),
            file_path: "src/math.rs".to_string(),
            line_start: 3,
            line_end: 5,
            docstring: Some("Adds two numbers.".to_string()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 100,
            package: None,
        }),
        NodeChange::Add(GraphNode {
            id: 2,
            hash: "fn_sub_hash00".to_string(),
            kind: NodeKind::Function,
            name: "subtract".to_string(),
            signature: "fn subtract(a: i32, b: i32) -> i32".to_string(),
            file_path: "src/math.rs".to_string(),
            line_start: 7,
            line_end: 9,
            docstring: Some("Subtracts b from a.".to_string()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 100,
            package: None,
        }),
        NodeChange::Add(GraphNode {
            id: 3,
            hash: "fn_mul_hash00".to_string(),
            kind: NodeKind::Function,
            name: "multiply".to_string(),
            signature: "fn multiply(a: i32, b: i32) -> i32".to_string(),
            file_path: "src/math.rs".to_string(),
            line_start: 11,
            line_end: 13,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 100,
            package: None,
        }),
    ];
    store.update_nodes(nodes).expect("Failed to insert nodes");

    let edges = vec![
        EdgeChange::Add(make_edge(1, 100, 1, EdgeKind::Contains, "src/math.rs", 3)),
        EdgeChange::Add(make_edge(2, 100, 2, EdgeKind::Contains, "src/math.rs", 7)),
        EdgeChange::Add(make_edge(3, 100, 3, EdgeKind::Contains, "src/math.rs", 11)),
        EdgeChange::Add(make_edge(4, 1, 2,   EdgeKind::Calls,    "src/math.rs", 4)),
    ];
    store.update_edges(edges).expect("Failed to insert edges");

    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_graph_has_expected_modules() {
        let store = create_test_graph();
        let modules = store.get_all_modules();
        assert_eq!(modules.len(), 5, "Expected 5 modules");
    }

    #[test]
    fn test_create_test_graph_has_expected_functions() {
        let store = create_test_graph();
        // Check one module's functions
        let auth_fns = store.get_nodes_in_file("src/auth.rs");
        // auth.rs has 1 module + 4 functions = 5 nodes
        assert_eq!(auth_fns.len(), 5, "Expected 5 nodes in src/auth.rs");
    }

    #[test]
    fn test_create_empty_graph_is_empty() {
        let store = create_empty_graph();
        let modules = store.get_all_modules();
        assert_eq!(modules.len(), 0, "Expected 0 modules in empty graph");
    }

    #[test]
    fn test_create_single_module_graph() {
        let store = create_single_module_graph();
        let modules = store.get_all_modules();
        assert_eq!(modules.len(), 1, "Expected 1 module");

        let fns = store.get_nodes_in_file("src/math.rs");
        assert_eq!(fns.len(), 4, "Expected 4 nodes (1 module + 3 functions)");

        // Check the add -> subtract call edge
        let edges = store.get_edges(1, keel_core::types::EdgeDirection::Outgoing);
        assert_eq!(edges.len(), 1, "Expected 1 outgoing edge from add");
        assert_eq!(edges[0].target_id, 2, "add should call subtract");
    }
}
