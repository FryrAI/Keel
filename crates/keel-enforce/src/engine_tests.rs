use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_parsers::resolver::Definition;

// Shared fixtures used by the engine_tests_* submodules below. The suite is
// split by topic (rather than one large file) to stay under the project's
// file-size cap; each submodule pulls these in via `use super::*;`.

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn make_call_edge(id: u64, src: u64, tgt: u64, file: &str) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line: 15,
        confidence: 1.0,
    }
}

fn make_definition(name: &str, sig: &str, body: &str, file: &str) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        body_text: body.to_string(),
    }
}

#[path = "engine_tests_batch_suppress.rs"]
mod batch_suppress;
#[path = "engine_tests_circuit_breaker.rs"]
mod circuit_breaker_reset;
#[path = "engine_tests_e001.rs"]
mod e001;
#[path = "engine_tests_e002_e003.rs"]
mod e002_e003;
#[path = "engine_tests_e004_misc.rs"]
mod e004_misc;
#[path = "engine_tests_economy.rs"]
mod economy;
