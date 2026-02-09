// Tests for GraphNode creation and NodeKind variants (Spec 000 - Graph Schema)
//
// use keel_core::graph::{GraphNode, NodeKind, ModuleId};

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Function should populate all required fields.
fn test_create_function_node() {
    // GIVEN a canonical signature, body hash, and function metadata
    // WHEN a GraphNode is created with NodeKind::Function
    // THEN the node has a valid hash, correct kind, and all fields populated
}

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Class should store class-level metadata.
fn test_create_class_node() {
    // GIVEN a class name, signature, and body
    // WHEN a GraphNode is created with NodeKind::Class
    // THEN the node has kind Class with correct name and hash
}

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Module should represent a file-level module.
fn test_create_module_node() {
    // GIVEN a file path and module-level metadata
    // WHEN a GraphNode is created with NodeKind::Module
    // THEN the node represents the entire module with correct file association
}

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Method should link to its parent class.
fn test_create_method_node() {
    // GIVEN a method signature and parent class reference
    // WHEN a GraphNode is created with NodeKind::Method
    // THEN the node has kind Method and references its parent class
}

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Interface should capture interface contracts.
fn test_create_interface_node() {
    // GIVEN an interface definition with method signatures
    // WHEN a GraphNode is created with NodeKind::Interface
    // THEN the node captures all interface method signatures
}

#[test]
#[ignore = "Not yet implemented"]
/// Creating a GraphNode with NodeKind::Trait should capture trait bounds and methods.
fn test_create_trait_node() {
    // GIVEN a trait definition with methods and bounds
    // WHEN a GraphNode is created with NodeKind::Trait
    // THEN the node captures trait methods and any supertraits
}

#[test]
#[ignore = "Not yet implemented"]
/// A GraphNode created without a docstring should have None for the docstring field.
fn test_node_without_docstring() {
    // GIVEN a function with no docstring
    // WHEN a GraphNode is created from it
    // THEN the docstring field is None
}

#[test]
#[ignore = "Not yet implemented"]
/// A GraphNode created with a docstring should store it and include it in hash computation.
fn test_node_with_docstring() {
    // GIVEN a function with a docstring
    // WHEN a GraphNode is created from it
    // THEN the docstring field is Some and affects the node hash
}

#[test]
#[ignore = "Not yet implemented"]
/// A GraphNode should track its module_id to associate with its containing module.
fn test_node_module_id_association() {
    // GIVEN a function inside a specific module
    // WHEN a GraphNode is created for that function
    // THEN the module_id field references the correct module
}

#[test]
#[ignore = "Not yet implemented"]
/// A GraphNode with external_endpoints should track API surface information.
fn test_node_with_external_endpoints() {
    // GIVEN a function that serves as an external API endpoint
    // WHEN a GraphNode is created with external_endpoints populated
    // THEN the endpoint metadata is accessible on the node
}
