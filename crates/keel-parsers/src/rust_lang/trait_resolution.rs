//! Heuristic-based (Tier 2) trait resolution for Rust.
//!
//! Extracts generic bounds, where clause bounds, supertrait hierarchies,
//! and associated type implementations from Rust source text.

use std::collections::HashMap;

use crate::resolver::ResolvedEdge;

use super::TraitImpl;

/// Parse `fn foo<T: Trait + OtherTrait>(...)` patterns.
/// Returns a map from type parameter name to list of trait bounds.
pub fn extract_generic_bounds(content: &str) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // Match function signatures with generic bounds
        if !trimmed.contains('<') || !trimmed.contains('>') {
            continue;
        }
        // Look for fn declarations or impl blocks with generic params
        let is_fn = trimmed.starts_with("fn ")
            || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub(crate) fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub async fn ");

        if !is_fn {
            continue;
        }

        // Extract the content between < and >
        if let Some(bounds_str) = extract_angle_bracket_content(trimmed) {
            parse_type_params_with_bounds(bounds_str, &mut result);
        }
    }

    result
}

/// Parse `where T: Trait + Send` patterns.
/// Returns a map from type parameter name to list of trait bounds.
pub fn extract_where_clause_bounds(content: &str) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Match lines starting with "where" or containing "where " after fn sig
        let where_content = if trimmed.starts_with("where ") || trimmed == "where" {
            // Collect the where clause which may span multiple lines
            collect_where_clause(&lines, i)
        } else if let Some(pos) = trimmed.find(") where ") {
            // Inline where clause: fn foo<T>(x: T) where T: Trait {
            let after_where = &trimmed[pos + 8..];
            after_where
                .trim_end_matches('{')
                .trim()
                .to_string()
        } else {
            continue;
        };

        parse_where_clause_items(&where_content, &mut result);
    }

    result
}

/// Parse `trait A: B + C` to build supertrait hierarchy map.
pub fn extract_supertrait_bounds(content: &str) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let rest = if let Some(s) = trimmed.strip_prefix("pub trait ") { s }
            else if let Some(s) = trimmed.strip_prefix("trait ") { s }
            else { continue };
        let name_end = rest.find(['<', ':', '{', ' '])
            .unwrap_or(rest.len());
        let trait_name = rest[..name_end].trim();
        if trait_name.is_empty() { continue; }
        let after_name = &rest[name_end..];
        let after_generics = if after_name.starts_with('<') {
            skip_angle_brackets(after_name).unwrap_or(after_name)
        } else { after_name };
        if let Some(pos) = after_generics.find(':') {
            let bs = after_generics[pos + 1..].split('{').next().unwrap_or("")
                .split("where").next().unwrap_or("").trim();
            let bounds: Vec<String> = bs.split('+')
                .map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).collect();
            if !bounds.is_empty() { result.insert(trait_name.to_string(), bounds); }
        }
    }
    result
}

/// Extract `type Output = String;` from impl blocks.
/// Returns Vec<(trait_name, assoc_type_name, concrete_type)>.
pub fn extract_associated_type_impls(content: &str) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    let (mut current_trait, mut brace_depth, mut in_impl): (Option<String>, i32, bool) =
        (None, 0, false);
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((tn, _)) = parse_impl_trait_for(trimmed) {
            current_trait = Some(tn);
            in_impl = false;
            brace_depth = 0;
        }
        for ch in trimmed.chars() {
            match ch {
                '{' => { brace_depth += 1; if current_trait.is_some() { in_impl = true; } }
                '}' => { brace_depth -= 1;
                    if in_impl && brace_depth <= 0 { current_trait = None; in_impl = false; } }
                _ => {}
            }
        }
        if in_impl {
            if let (Some(ref ct), Some((tn, c))) = (&current_trait, parse_associated_type_def(trimmed)) {
                result.push((ct.clone(), tn, c));
            }
        }
    }
    result
}

/// Resolve a generic method call via trait bounds + supertrait expansion.
/// Returns confidence 0.65 when a matching trait impl is found.
pub fn resolve_generic_method_call(
    receiver: &str, method: &str,
    generic_bounds: &HashMap<String, Vec<String>>,
    where_bounds: &HashMap<String, Vec<String>>,
    trait_impls: &[TraitImpl],
    supertrait_bounds: &HashMap<String, Vec<String>>,
    _file_path: &str,
) -> Option<ResolvedEdge> {
    let mut all_bounds: Vec<String> = Vec::new();
    if let Some(b) = generic_bounds.get(receiver) { all_bounds.extend(b.clone()); }
    if let Some(b) = where_bounds.get(receiver) { all_bounds.extend(b.clone()); }
    if all_bounds.is_empty() { return None; }

    // Expand via supertraits
    let mut expanded = all_bounds.clone();
    for bound in &all_bounds {
        expand_supertraits(bound, supertrait_bounds, &mut expanded);
    }

    // Check trait_impls for any trait in expanded bounds that has the method
    for trait_name in &expanded {
        if let Some(ti) = trait_impls.iter().find(|ti| {
            ti.trait_name == *trait_name && ti.methods.iter().any(|m| m == method)
        }) {
            return Some(ResolvedEdge {
                target_file: ti.file_path.clone(),
                target_name: method.to_string(),
                confidence: 0.65,
                resolution_tier: "tier2".into(),
            });
        }
    }
    None
}

// ---- Private helpers ----

/// Extract content between the first `<` and its matching `>`.
fn extract_angle_bracket_content(s: &str) -> Option<&str> {
    let start = s.find('<')? + 1;
    let mut depth = 1i32;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Skip over `<...>` generics, returning the rest of the string.
fn skip_angle_brackets(s: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[i + 1..].trim());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse type params like `T: Trait + Send, U: Clone` into the result map.
fn parse_type_params_with_bounds(params: &str, result: &mut HashMap<String, Vec<String>>) {
    for seg in split_respecting_brackets(params) {
        let seg = seg.trim();
        if let Some(pos) = seg.find(':') {
            let param = seg[..pos].trim();
            let bounds: Vec<String> = seg[pos + 1..].trim().split('+')
                .map(|b| { let b = b.trim(); b.find('<').map_or(b, |i| &b[..i]).to_string() })
                .filter(|b| !b.is_empty()).collect();
            if !bounds.is_empty() { result.entry(param.to_string()).or_default().extend(bounds); }
        }
    }
}

/// Split a string by commas, but respect nested `<>` brackets.
fn split_respecting_brackets(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&s[start..]);
    result
}

/// Collect a where clause that may span multiple lines.
fn collect_where_clause(lines: &[&str], start_idx: usize) -> String {
    let mut clause = String::new();
    for line in &lines[start_idx..] {
        let trimmed = line.trim();
        let part = if let Some(s) = trimmed.strip_prefix("where ") { s }
            else if trimmed == "where" { "" } else { trimmed };
        let part = part.trim_end_matches('{').trim();
        if !part.is_empty() {
            if !clause.is_empty() { clause.push_str(", "); }
            clause.push_str(part);
        }
        if trimmed.contains('{') { break; }
    }
    clause
}

/// Parse individual where clause items like "T: Trait + Send".
fn parse_where_clause_items(clause: &str, result: &mut HashMap<String, Vec<String>>) {
    for seg in split_respecting_brackets(clause) {
        let seg = seg.trim();
        if let Some(pos) = seg.find(':') {
            let param = seg[..pos].trim();
            let bounds: Vec<String> = seg[pos + 1..].trim().split('+')
                .map(|b| { let b = b.trim(); b.find('<').map_or(b, |i| &b[..i]).to_string() })
                .filter(|b| !b.is_empty()).collect();
            if !bounds.is_empty() { result.entry(param.to_string()).or_default().extend(bounds); }
        }
    }
}

/// Parse `impl Trait for Type` line into (trait_name, type_name).
fn parse_impl_trait_for(line: &str) -> Option<(String, String)> {
    let s = line.strip_prefix("impl ")?.trim();
    let for_pos = s.find(" for ")?;
    let trait_part = s[..for_pos].trim();
    let rest = s[for_pos + 5..].trim();
    let type_end = rest
        .find(['{', '<'])
        .or_else(|| rest.find(" where "))
        .unwrap_or(rest.len());
    let type_part = rest[..type_end].trim();
    if trait_part.is_empty() || type_part.is_empty() {
        return None;
    }
    let trait_name = trait_part.find('<').map_or(trait_part, |i| &trait_part[..i]);
    Some((trait_name.to_string(), type_part.to_string()))
}

/// Parse `type Name = ConcreteType;` from inside an impl block.
fn parse_associated_type_def(line: &str) -> Option<(String, String)> {
    let s = line.strip_prefix("type ")?.trim();
    let eq_pos = s.find('=')?;
    let name = s[..eq_pos].trim();
    let concrete = s[eq_pos + 1..]
        .trim()
        .trim_end_matches(';')
        .trim();
    if name.is_empty() || concrete.is_empty() {
        return None;
    }
    Some((name.to_string(), concrete.to_string()))
}

/// Recursively expand supertraits into the expanded list.
fn expand_supertraits(
    trait_name: &str,
    supertrait_bounds: &HashMap<String, Vec<String>>,
    expanded: &mut Vec<String>,
) {
    if let Some(supers) = supertrait_bounds.get(trait_name) {
        for s in supers {
            if !expanded.contains(s) {
                expanded.push(s.clone());
                expand_supertraits(s, supertrait_bounds, expanded);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_generic_bounds_basic() {
        let src = "pub fn run<T: Processor>(p: &T) {}";
        let bounds = extract_generic_bounds(src);
        assert_eq!(bounds.get("T"), Some(&vec!["Processor".to_string()]));
    }

    #[test]
    fn test_extract_generic_bounds_multiple() {
        let src = "fn process<T: Send + Sync, U: Clone>(a: T, b: U) {}";
        let bounds = extract_generic_bounds(src);
        assert!(bounds.get("T").unwrap().contains(&"Send".to_string()));
        assert!(bounds.get("T").unwrap().contains(&"Sync".to_string()));
        assert!(bounds.get("U").unwrap().contains(&"Clone".to_string()));
    }

    #[test]
    fn test_extract_where_clause() {
        let src = "fn check<T>(v: &T) where T: Validator + Send {\n}";
        let bounds = extract_where_clause_bounds(src);
        let t_bounds = bounds.get("T").unwrap();
        assert!(t_bounds.contains(&"Validator".to_string()));
        assert!(t_bounds.contains(&"Send".to_string()));
    }

    #[test]
    fn test_extract_supertrait_bounds_basic() {
        let src = "trait Advanced: Base + Send {}";
        let bounds = extract_supertrait_bounds(src);
        let adv = bounds.get("Advanced").unwrap();
        assert!(adv.contains(&"Base".to_string()));
        assert!(adv.contains(&"Send".to_string()));
    }

    #[test]
    fn test_extract_associated_type() {
        let src = r#"
impl Converter for StringConverter {
    type Output = String;
    fn convert(&self) -> String { "x".to_string() }
}
"#;
        let assoc = extract_associated_type_impls(src);
        assert_eq!(assoc.len(), 1);
        assert_eq!(assoc[0], ("Converter".to_string(), "Output".to_string(), "String".to_string()));
    }

    #[test]
    fn test_expand_supertraits() {
        let mut supers = HashMap::new();
        supers.insert("Advanced".to_string(), vec!["Base".to_string()]);
        supers.insert("Base".to_string(), vec!["Root".to_string()]);

        let mut expanded = vec!["Advanced".to_string()];
        expand_supertraits("Advanced", &supers, &mut expanded);
        assert!(expanded.contains(&"Base".to_string()));
        assert!(expanded.contains(&"Root".to_string()));
    }
}
