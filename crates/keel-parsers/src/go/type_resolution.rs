//! Go type-aware resolution: receiver methods, struct embedding, interfaces.

use std::collections::HashMap;

use crate::resolver::{ParseResult, ResolvedEdge};
use keel_core::types::NodeKind;

/// Receiver info extracted from a Go method signature.
#[derive(Debug, Clone)]
pub struct ReceiverInfo {
    pub type_name: String,
    pub is_pointer: bool,
    pub method_name: String,
}

/// Parsed interface with its required method names.
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub methods: Vec<String>,
    pub file_path: String,
}

/// Extract receiver from `(varname *Type)` or `(varname Type)` param text.
pub fn extract_receiver_from_params(
    receiver_text: &str,
    method_name: &str,
) -> Option<ReceiverInfo> {
    let trimmed = receiver_text.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let (type_name, is_pointer) = if parts[1].starts_with('*') {
        (parts[1][1..].to_string(), true)
    } else {
        (parts[1].to_string(), false)
    };
    Some(ReceiverInfo { type_name, is_pointer, method_name: method_name.to_string() })
}

/// Build type_name -> vec of (method_name, is_pointer_receiver) from content.
pub fn build_type_methods(
    result: &ParseResult,
    content: &str,
) -> HashMap<String, Vec<(String, bool)>> {
    let mut map: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for def in &result.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        if let Some(info) = extract_receiver_from_content(content, &def.name, def.line_start) {
            map.entry(info.type_name).or_default().push((info.method_name, info.is_pointer));
        }
    }
    map
}

/// Extract receiver info by scanning the raw source line of a method definition.
fn extract_receiver_from_content(
    content: &str,
    method_name: &str,
    line_start: u32,
) -> Option<ReceiverInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line_start as usize).saturating_sub(1);
    let line = lines.get(idx)?;
    let func_pos = line.find("func ")?;
    let after_func = &line[func_pos + 5..];
    if !after_func.starts_with('(') {
        return None;
    }
    let close_paren = after_func.find(')')?;
    let parts: Vec<&str> = after_func[1..close_paren].split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let (type_name, is_pointer) = if parts[1].starts_with('*') {
        (parts[1][1..].to_string(), true)
    } else {
        (parts[1].to_string(), false)
    };
    Some(ReceiverInfo { type_name, is_pointer, method_name: method_name.to_string() })
}

/// Extract struct embeddings. Returns outer_type -> vec of embedded_type_names.
pub fn extract_embeddings(content: &str) -> HashMap<String, Vec<String>> {
    let mut embeddings: HashMap<String, Vec<String>> = HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_struct: Option<String> = None;
    let mut brace_depth = 0i32;

    for line in &lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("type ") {
            if let Some(struct_pos) = rest.find(" struct") {
                let name = rest[..struct_pos].trim();
                if !name.is_empty() && rest[struct_pos..].contains('{') {
                    current_struct = Some(name.to_string());
                    brace_depth = 1;
                    continue;
                }
            }
        }
        if current_struct.is_some() {
            brace_depth += trimmed.matches('{').count() as i32;
            brace_depth -= trimmed.matches('}').count() as i32;
            if brace_depth <= 0 {
                current_struct = None;
                continue;
            }
            if is_embedded_field(trimmed) {
                if let Some(embedded_type) = parse_embedded_type(trimmed) {
                    if let Some(ref struct_name) = current_struct {
                        embeddings.entry(struct_name.clone()).or_default().push(embedded_type);
                    }
                }
            }
        }
    }
    embeddings
}

fn is_embedded_field(trimmed: &str) -> bool {
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
        return false;
    }
    let t = trimmed.trim_start_matches('*');
    !t.is_empty()
        && t.chars().next().is_some_and(|c| c.is_uppercase())
        && t.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn parse_embedded_type(trimmed: &str) -> Option<String> {
    let t = trimmed.trim_start_matches('*');
    if t.is_empty() { return None; }
    if t.chars().next()?.is_uppercase() && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(t.to_string())
    } else {
        None
    }
}

/// Extract interface definitions and their method signatures from Go source.
pub fn extract_interfaces(
    result: &ParseResult,
    content: &str,
    file_path: &str,
) -> Vec<InterfaceInfo> {
    let mut interfaces = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    for def in &result.definitions {
        if def.kind != NodeKind::Class { continue; }
        let idx = (def.line_start as usize).saturating_sub(1);
        if idx >= lines.len() { continue; }
        if !lines[idx].contains("interface") { continue; }
        let methods = extract_interface_methods(content, def.line_start, def.line_end);
        interfaces.push(InterfaceInfo {
            name: def.name.clone(),
            methods,
            file_path: file_path.to_string(),
        });
    }
    interfaces
}

fn extract_interface_methods(content: &str, line_start: u32, line_end: u32) -> Vec<String> {
    let mut methods = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let start = line_start as usize;
    let end = (line_end as usize).saturating_sub(1).min(lines.len());
    for i in start..end {
        let trimmed = lines[i].trim();
        if let Some(paren_pos) = trimmed.find('(') {
            let name = trimmed[..paren_pos].trim();
            if !name.is_empty()
                && !name.contains(' ')
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                methods.push(name.to_string());
            }
        }
    }
    methods
}

/// Find types satisfying an interface through structural typing.
pub fn find_interface_satisfiers(
    iface: &InterfaceInfo,
    type_methods: &HashMap<String, Vec<(String, bool)>>,
) -> Vec<(String, f64)> {
    if iface.methods.is_empty() {
        return type_methods.keys().map(|t| (t.clone(), 0.30)).collect();
    }
    let mut satisfiers = Vec::new();
    for (type_name, methods) in type_methods {
        let names: Vec<&str> = methods.iter().map(|(n, _)| n.as_str()).collect();
        if iface.methods.iter().all(|im| names.contains(&im.as_str())) {
            satisfiers.push((type_name.clone(), 0.40));
        }
    }
    satisfiers
}

/// Resolve a method call on a receiver using type-aware heuristics.
pub fn resolve_receiver_method(
    receiver: &str,
    method_name: &str,
    file_path: &str,
    type_methods: &HashMap<String, Vec<(String, bool)>>,
    embeddings: &HashMap<String, Vec<String>>,
    interfaces: &[InterfaceInfo],
) -> Option<ResolvedEdge> {
    // 1. Direct type method lookup
    if let Some(methods) = type_methods.get(receiver) {
        if methods.iter().any(|(n, _)| n == method_name) {
            return Some(ResolvedEdge {
                target_file: file_path.to_string(),
                target_name: method_name.to_string(),
                confidence: 0.70,
                resolution_tier: "tier2_heuristic".into(),
            });
        }
    }

    // 2. Struct embedding: check promoted methods (outer wins on collision)
    if let Some(edge) = resolve_embedded_method(
        receiver, method_name, file_path, type_methods, embeddings, &mut Vec::new(),
    ) {
        return Some(edge);
    }

    // 3. Interface dispatch
    for iface in interfaces {
        if iface.name == receiver && iface.methods.contains(&method_name.to_string()) {
            let satisfiers = find_interface_satisfiers(iface, type_methods);
            let confidence = if iface.methods.is_empty() {
                0.30
            } else if satisfiers.is_empty() {
                0.35
            } else {
                0.40
            };
            return Some(ResolvedEdge {
                target_file: file_path.to_string(),
                target_name: method_name.to_string(),
                confidence,
                resolution_tier: "tier2_heuristic".into(),
            });
        }
    }

    None
}

fn resolve_embedded_method(
    type_name: &str,
    method_name: &str,
    file_path: &str,
    type_methods: &HashMap<String, Vec<(String, bool)>>,
    embeddings: &HashMap<String, Vec<String>>,
    visited: &mut Vec<String>,
) -> Option<ResolvedEdge> {
    if visited.contains(&type_name.to_string()) { return None; }
    visited.push(type_name.to_string());
    if let Some(embedded_types) = embeddings.get(type_name) {
        for embedded in embedded_types {
            if let Some(methods) = type_methods.get(embedded.as_str()) {
                if methods.iter().any(|(n, _)| n == method_name) {
                    return Some(ResolvedEdge {
                        target_file: file_path.to_string(),
                        target_name: method_name.to_string(),
                        confidence: 0.65,
                        resolution_tier: "tier2_heuristic".into(),
                    });
                }
            }
            if let Some(edge) = resolve_embedded_method(
                embedded, method_name, file_path, type_methods, embeddings, visited,
            ) {
                return Some(edge);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_receiver_pointer() {
        let info = extract_receiver_from_params("(s *Service)", "Process").unwrap();
        assert_eq!(info.type_name, "Service");
        assert!(info.is_pointer);
    }

    #[test]
    fn test_extract_receiver_value() {
        let info = extract_receiver_from_params("(s Service)", "String").unwrap();
        assert_eq!(info.type_name, "Service");
        assert!(!info.is_pointer);
    }

    #[test]
    fn test_is_embedded_field() {
        assert!(is_embedded_field("Logger"));
        assert!(!is_embedded_field("name string"));
        assert!(!is_embedded_field("// comment"));
        assert!(!is_embedded_field(""));
    }

    #[test]
    fn test_extract_embeddings() {
        let source = "type Inner struct {\n    value int\n}\n\ntype Outer struct {\n    Inner\n    name string\n}\n";
        let emb = extract_embeddings(source);
        assert!(emb.contains_key("Outer"));
        assert_eq!(emb["Outer"], vec!["Inner".to_string()]);
    }
}
