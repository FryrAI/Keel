// Tests for Python tree-sitter parser (Spec 001 - Tree-sitter Foundation)
//
// use keel_parsers::python::PyResolver;
// use keel_core::types::{GraphNode, NodeKind};

#[test]
#[ignore = "Not yet implemented"]
/// Parsing a Python file with a def statement should produce a Function node.
fn test_py_parse_function_def() {
    // GIVEN a Python file containing `def process(data: list) -> dict:`
    // WHEN the Python parser processes the file
    // THEN a GraphNode with NodeKind::Function and name "process" is produced
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing a Python class should produce a Class node with Method children.
fn test_py_parse_class_with_methods() {
    // GIVEN a Python file with a class containing __init__ and 2 other methods
    // WHEN the Python parser processes the file
    // THEN a Class node and 3 Method nodes are produced
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing Python import statements should produce Import edges.
fn test_py_parse_import_statements() {
    // GIVEN a Python file with `from utils.parser import parse_json`
    // WHEN the Python parser processes the file
    // THEN an Imports edge is created for "parse_json" from "utils.parser"
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing Python call sites should produce Calls edges.
fn test_py_parse_call_sites() {
    // GIVEN a Python file where function A calls function B
    // WHEN the Python parser processes the file
    // THEN a Calls edge from A to B is produced
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing decorated Python functions should capture the decorator metadata.
fn test_py_parse_decorated_function() {
    // GIVEN a Python file with `@app.route("/api")` decorated function
    // WHEN the Python parser processes the file
    // THEN the function node captures decorator information
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing Python docstrings should be captured in the node's docstring field.
fn test_py_parse_docstring() {
    // GIVEN a Python function with a triple-quoted docstring
    // WHEN the Python parser processes the file
    // THEN the GraphNode's docstring field contains the docstring text
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing Python async functions should be handled correctly.
fn test_py_parse_async_function() {
    // GIVEN a Python file with `async def fetch(url: str) -> Response:`
    // WHEN the Python parser processes the file
    // THEN a Function node is produced with the async attribute set
}

#[test]
#[ignore = "Not yet implemented"]
/// Parsing Python nested functions should create separate nodes with containment edges.
fn test_py_parse_nested_functions() {
    // GIVEN a Python function containing an inner function definition
    // WHEN the Python parser processes the file
    // THEN both functions have nodes and a Contains edge links outer to inner
}
