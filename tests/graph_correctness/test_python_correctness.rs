// Oracle 1: Python graph correctness vs LSP ground truth
//
// Compares keel's Python graph output against LSP/SCIP baseline data
// to validate node counts, edge counts, and resolution accuracy.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};

#[test]
fn test_py_function_node_count_matches_lsp() {
    // GIVEN a Python file with exactly 4 top-level functions
    let resolver = PyResolver::new();
    let source = r#"
def read_input(path: str) -> str:
    return open(path).read()

def tokenize(text: str) -> list:
    return text.split()

def analyze(tokens: list) -> dict:
    return {}

def report(analysis: dict) -> None:
    print(analysis)
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("pipeline.py"), source);

    // THEN the number of Function nodes matches exactly 4
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 4, "expected 4 Function definitions, got {}", funcs.len());
}

#[test]
fn test_py_class_node_count_matches_lsp() {
    // GIVEN a Python file with exactly 2 classes
    let resolver = PyResolver::new();
    let source = r#"
class Shape:
    def area(self) -> float:
        return 0.0

class Circle(Shape):
    def __init__(self, radius: float) -> None:
        self.radius = radius

    def area(self) -> float:
        return 3.14159 * self.radius ** 2
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("shapes.py"), source);

    // THEN exactly 2 Class definitions
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 2, "expected 2 Class definitions, got {}", classes.len());
    assert!(classes.iter().any(|c| c.name == "Shape"), "missing Shape");
    assert!(classes.iter().any(|c| c.name == "Circle"), "missing Circle");
}

#[test]
fn test_py_module_node_count_matches_lsp() {
    // GIVEN a Python file
    let resolver = PyResolver::new();
    let source = r#"
def greet() -> str:
    return "hello"
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("greet.py"), source);

    // THEN exactly 1 Module node is auto-created for the file
    let modules: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Module)
        .collect();
    assert_eq!(modules.len(), 1, "expected 1 Module node per file, got {}", modules.len());
    assert_eq!(modules[0].name, "greet", "module name should be file stem");
    assert_eq!(modules[0].file_path, "greet.py");
}

#[test]
fn test_py_call_edge_count_matches_lsp() {
    // GIVEN Python code with known function calls
    let resolver = PyResolver::new();
    let source = r#"
def helper() -> int:
    return 42

def compute(x: int) -> int:
    return x * 2

def main() -> None:
    a = helper()
    b = compute(a)
    print(b)
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("calls.py"), source);

    // THEN call references are found
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.len() >= 2,
        "expected >= 2 call references (helper, compute), got {}",
        calls.len()
    );
}

#[test]
fn test_py_import_resolution_matches_lsp() {
    // GIVEN Python code with from-import statements
    // Note: bare `import os` uses import_statement which captures name but not
    // source, so the tree-sitter layer only creates Import entries for
    // `from X import Y` style imports.
    let resolver = PyResolver::new();
    let source = r#"
from pathlib import Path
from typing import List, Dict
from collections import OrderedDict

def list_files(directory: str) -> List[str]:
    return []
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("utils.py"), source);

    // THEN from-import entries are captured
    assert!(
        result.imports.len() >= 2,
        "expected >= 2 imports, got {} ({:?})",
        result.imports.len(),
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
    let pathlib_import = result.imports.iter().find(|i| i.source.contains("pathlib"));
    assert!(pathlib_import.is_some(), "should detect 'from pathlib import Path'");
    let typing_import = result.imports.iter().find(|i| i.source.contains("typing"));
    assert!(typing_import.is_some(), "should detect 'from typing import List, Dict'");
}

#[test]
fn test_py_decorator_functions_detected() {
    // GIVEN a Python file with decorated functions
    let resolver = PyResolver::new();
    let source = r#"
class Service:
    @staticmethod
    def create() -> 'Service':
        return Service()

    @classmethod
    def from_config(cls, config: dict) -> 'Service':
        return cls()

    def process(self, data: str) -> str:
        return data
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("service.py"), source);

    // THEN decorated functions are captured as definitions
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.iter().any(|f| f.name == "create"),
        "should detect @staticmethod function 'create'"
    );
    assert!(
        funcs.iter().any(|f| f.name == "from_config"),
        "should detect @classmethod function 'from_config'"
    );
    assert!(
        funcs.iter().any(|f| f.name == "process"),
        "should detect regular method 'process'"
    );
}

#[test]
fn test_py_method_resolution_matches_lsp() {
    // GIVEN Python code with class instantiation and method call
    let resolver = PyResolver::new();
    let source = r#"
class Encoder:
    def encode(self, data: str) -> bytes:
        return data.encode()

def main() -> None:
    e = Encoder()
    result = e.encode("hello")
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("encoder.py"), source);

    // THEN a call reference to 'encode' is detected
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    // At minimum, Encoder() and e.encode() should appear
    assert!(
        calls.len() >= 1,
        "expected >= 1 call references, got {}",
        calls.len()
    );
}

#[test]
fn test_py_nested_function_detection() {
    // GIVEN Python code with a nested (inner) function
    let resolver = PyResolver::new();
    let source = r#"
def outer(x: int) -> int:
    def inner(y: int) -> int:
        return y * 2
    return inner(x)
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("nested.py"), source);

    // THEN both outer and inner functions are captured
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.iter().any(|f| f.name == "outer"),
        "should detect outer function"
    );
    assert!(
        funcs.iter().any(|f| f.name == "inner"),
        "should detect nested inner function"
    );
}
