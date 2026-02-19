// Tests for GraphNode creation and NodeKind variants (Spec 000 - Graph Schema)

use keel_core::types::{ExternalEndpoint, GraphNode, NodeKind};

#[test]
/// Creating a GraphNode with NodeKind::Function should populate all required fields.
fn test_create_function_node() {
    let node = GraphNode {
        id: 1,
        hash: "abc12345678".into(),
        kind: NodeKind::Function,
        name: "my_func".into(),
        signature: "my_func(x: i32) -> i32".into(),
        file_path: "src/lib.rs".into(),
        line_start: 10,
        line_end: 15,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Function);
    assert_eq!(node.name, "my_func");
    assert!(!node.hash.is_empty());
    assert_eq!(node.line_start, 10);
    assert_eq!(node.line_end, 15);
    assert!(node.is_public);
    assert!(node.type_hints_present);
}

#[test]
/// Creating a GraphNode with NodeKind::Class should store class-level metadata.
fn test_create_class_node() {
    let node = GraphNode {
        id: 2,
        hash: "cls12345678".into(),
        kind: NodeKind::Class,
        name: "UserService".into(),
        signature: "class UserService".into(),
        file_path: "src/services.ts".into(),
        line_start: 1,
        line_end: 50,
        docstring: Some("User management service".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Class);
    assert_eq!(node.name, "UserService");
    assert!(!node.hash.is_empty());
    assert!(node.has_docstring);
}

#[test]
/// Creating a GraphNode with NodeKind::Module should represent a file-level module.
fn test_create_module_node() {
    let node = GraphNode {
        id: 3,
        hash: "mod12345678".into(),
        kind: NodeKind::Module,
        name: "utils".into(),
        signature: "module utils".into(),
        file_path: "src/utils.ts".into(),
        line_start: 1,
        line_end: 200,
        docstring: None,
        is_public: true,
        type_hints_present: false,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Module);
    assert_eq!(node.name, "utils");
    assert_eq!(node.file_path, "src/utils.ts");
}

#[test]
/// Creating a GraphNode for a method should use NodeKind::Function.
/// Note: NodeKind has 3 variants (Module, Class, Function). Methods are Function.
fn test_create_method_node() {
    let node = GraphNode {
        id: 4,
        hash: "mth12345678".into(),
        kind: NodeKind::Function,
        name: "get_user".into(),
        signature: "get_user(self, id: str) -> User".into(),
        file_path: "src/services.py".into(),
        line_start: 15,
        line_end: 25,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 2,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Function);
    assert_eq!(node.name, "get_user");
    assert_eq!(node.module_id, 2);
}

#[test]
/// Creating a GraphNode for an interface should use NodeKind::Class.
fn test_create_interface_node() {
    let node = GraphNode {
        id: 5,
        hash: "ifc12345678".into(),
        kind: NodeKind::Class,
        name: "Repository".into(),
        signature: "interface Repository".into(),
        file_path: "src/repo.ts".into(),
        line_start: 1,
        line_end: 10,
        docstring: Some("Data access interface".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Class);
    assert_eq!(node.name, "Repository");
    assert!(node.signature.contains("interface"));
}

#[test]
/// Creating a GraphNode for a trait should use NodeKind::Class.
fn test_create_trait_node() {
    let node = GraphNode {
        id: 6,
        hash: "trt12345678".into(),
        kind: NodeKind::Class,
        name: "LanguageResolver".into(),
        signature: "trait LanguageResolver".into(),
        file_path: "src/resolver.rs".into(),
        line_start: 1,
        line_end: 30,
        docstring: Some("Core resolver trait".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert_eq!(node.kind, NodeKind::Class);
    assert_eq!(node.name, "LanguageResolver");
    assert!(node.signature.contains("trait"));
    assert!(node.has_docstring);
}

#[test]
/// A GraphNode created without a docstring should have None for the docstring field.
fn test_node_without_docstring() {
    let node = GraphNode {
        id: 7,
        hash: "ndc12345678".into(),
        kind: NodeKind::Function,
        name: "helper".into(),
        signature: "helper()".into(),
        file_path: "src/utils.rs".into(),
        line_start: 1,
        line_end: 3,
        docstring: None,
        is_public: false,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert!(node.docstring.is_none());
    assert!(!node.has_docstring);
}

#[test]
/// A GraphNode created with a docstring should store it.
fn test_node_with_docstring() {
    let node_with = GraphNode {
        id: 8,
        hash: "doc12345678".into(),
        kind: NodeKind::Function,
        name: "process".into(),
        signature: "process(data: Vec<u8>) -> Result<()>".into(),
        file_path: "src/lib.rs".into(),
        line_start: 1,
        line_end: 10,
        docstring: Some("Process input data".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    assert!(node_with.docstring.is_some());
    assert_eq!(node_with.docstring.as_deref(), Some("Process input data"));
    assert!(node_with.has_docstring);
}

#[test]
/// A GraphNode should track its module_id to associate with its containing module.
fn test_node_module_id_association() {
    let module_node = GraphNode {
        id: 100,
        hash: "modA1234567".into(),
        kind: NodeKind::Module,
        name: "utils".into(),
        signature: "module utils".into(),
        file_path: "src/utils.ts".into(),
        line_start: 1,
        line_end: 100,
        docstring: None,
        is_public: true,
        type_hints_present: false,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };

    let func_node = GraphNode {
        id: 101,
        hash: "fnA12345678".into(),
        kind: NodeKind::Function,
        name: "parse".into(),
        signature: "parse(input: string): string".into(),
        file_path: "src/utils.ts".into(),
        line_start: 5,
        line_end: 10,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: module_node.id,
        package: None,
    };

    assert_eq!(func_node.module_id, 100);
    assert_eq!(func_node.module_id, module_node.id);
}

#[test]
/// A GraphNode with external_endpoints should track API surface information.
fn test_node_with_external_endpoints() {
    let endpoint = ExternalEndpoint {
        kind: "HTTP".into(),
        method: "GET".into(),
        path: "/api/users".into(),
        direction: "serves".into(),
    };

    let node = GraphNode {
        id: 10,
        hash: "api12345678".into(),
        kind: NodeKind::Function,
        name: "get_users".into(),
        signature: "get_users(req: Request) -> Response".into(),
        file_path: "src/handlers.rs".into(),
        line_start: 20,
        line_end: 35,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![endpoint],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };

    assert_eq!(node.external_endpoints.len(), 1);
    assert_eq!(node.external_endpoints[0].kind, "HTTP");
    assert_eq!(node.external_endpoints[0].method, "GET");
    assert_eq!(node.external_endpoints[0].path, "/api/users");
    assert_eq!(node.external_endpoints[0].direction, "serves");
}
