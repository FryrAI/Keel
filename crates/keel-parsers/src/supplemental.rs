//! Supplemental structural parsing via Typst syntax, TypeScript extraction,
//! tree-sitter Bash, and PostgreSQL sqlparser into Keel's standard graph facts.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use keel_core::types::NodeKind;
use typst_syntax::ast::{self, AstNode, Expr as TypstExpr, LetBindingKind};
use typst_syntax::{LinkedNode, SyntaxKind, SyntaxNode};

use crate::boundary::BoundaryLiterals;
use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseCache, ParseResult, Reference,
    ReferenceKind, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;
use crate::typescript::TsResolver;

#[path = "supplemental/sql.rs"]
mod sql;
use sql::parse_sql;

/// One resolver for Keel's supplemental source formats.
pub struct SupplementalResolver {
    bash: Mutex<TreeSitterParser>,
    astro: TsResolver,
    cache: ParseCache,
}

impl SupplementalResolver {
    /// Build a Tier-1 resolver without project configuration.
    pub fn new() -> Self {
        Self {
            bash: Mutex::new(TreeSitterParser::new()),
            astro: TsResolver::new(),
            cache: ParseCache::default(),
        }
    }

    /// Build a resolver whose Astro imports honor the project's tsconfig.
    pub fn with_project_root(root: &Path) -> Self {
        Self {
            bash: Mutex::new(TreeSitterParser::new()),
            astro: TsResolver::with_project_root(root),
            cache: ParseCache::default(),
        }
    }

    /// Install known boundary names before the first Bash/Astro parse.
    pub fn with_boundary_literals(self, keys: Arc<HashSet<String>>) -> Self {
        self.bash
            .lock()
            .unwrap()
            .set_boundary_literals(keys.clone());
        self.astro.set_boundary_literals(keys);
        self
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let result = match path.extension().and_then(|ext| ext.to_str()) {
            Some("typ") => parse_typst(path, content),
            Some("astro") => self.parse_astro(path, content),
            Some("sh" | "bash" | "bats") => self.parse_bash(path, content),
            Some("sql") => parse_sql(path, content),
            _ => empty_result(),
        };
        self.cache.insert(path, result.clone());
        result
    }

    fn parse_bash(&self, path: &Path, content: &str) -> ParseResult {
        let mut result = match self.bash.lock().unwrap().parse_file("bash", path, content) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("keel: warning: failed to parse {}: {error}", path.display());
                return empty_result();
            }
        };
        for definition in &mut result.definitions {
            definition.is_public = false;
            // Bash is dynamically typed; typed-language annotation checks do
            // not apply, just as they do not apply to Typst or SQL.
            definition.type_hints_present = true;
        }
        result.imports.extend(bash_imports(path, content));
        result
    }

    fn parse_astro(&self, path: &Path, content: &str) -> ParseResult {
        self.parse_astro_with(&self.astro, path, content)
    }

    /// Parse Astro with an already-configured TypeScript resolver.
    pub fn parse_astro_with(
        &self,
        resolver: &TsResolver,
        path: &Path,
        content: &str,
    ) -> ParseResult {
        let source = astro_script_source(content);
        let mut result = resolver.parse_file(path, &source);
        let names: HashSet<String> = result
            .definitions
            .iter()
            .map(|definition| definition.name.clone())
            .chain(
                result
                    .imports
                    .iter()
                    .flat_map(|import| import.imported_names.iter().cloned()),
            )
            .collect();
        result.references.extend(astro_template_references(
            content,
            &source,
            &names,
            &path.to_string_lossy(),
        ));
        result
    }
}

impl Default for SupplementalResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageResolver for SupplementalResolver {
    fn language(&self) -> &str {
        "supplemental"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["typ", "astro", "sh", "bash", "bats", "sql"]
    }

    fn parse_file(&self, path: &Path, content: &str) -> ParseResult {
        self.parse_and_cache(path, content)
    }

    fn resolve_definitions(&self, file: &Path) -> Vec<Definition> {
        self.cache.definitions_for(file)
    }

    fn resolve_references(&self, file: &Path) -> Vec<Reference> {
        self.cache.references_for(file)
    }

    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge> {
        let result = self.cache.get(Path::new(&call_site.file_path))?;
        result
            .definitions
            .iter()
            .find(|definition| definition.name == call_site.callee_name)
            .map(|definition| ResolvedEdge {
                target_file: call_site.file_path.clone(),
                target_name: definition.name.clone(),
                confidence: 0.9,
                resolution_tier: "tier1".into(),
            })
    }
}

fn empty_result() -> ParseResult {
    ParseResult {
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
    }
}

fn supplemental_definition(
    name: String,
    kind: NodeKind,
    signature: String,
    path: &Path,
    lines: (u32, u32),
    body_text: String,
) -> Definition {
    Definition {
        name,
        kind,
        signature,
        file_path: path.to_string_lossy().to_string(),
        line_start: lines.0,
        line_end: lines.1,
        docstring: None,
        is_public: false,
        type_hints_present: true,
        body_text,
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
        complexity: 1,
        is_trivial_wrapper_body: false,
    }
}

fn parse_typst(path: &Path, content: &str) -> ParseResult {
    let root = typst_syntax::parse(content);
    if root.erroneous() {
        eprintln!(
            "keel: warning: {} contains Typst syntax errors; extracted valid structure only",
            path.display()
        );
    }
    let mut result = empty_result();
    visit_typst(LinkedNode::new(&root), path, content, &mut result);
    result
}

fn visit_typst(node: LinkedNode<'_>, path: &Path, content: &str, result: &mut ParseResult) {
    match node.kind() {
        SyntaxKind::LetBinding => {
            if let Some(binding) = node.get().cast::<ast::LetBinding>() {
                if let (LetBindingKind::Closure(name), Some(TypstExpr::Closure(closure))) =
                    (binding.kind(), binding.init())
                {
                    let params_node = closure.params();
                    let mut params = syntax_text(params_node.to_untyped());
                    if params_node
                        .children()
                        .any(|param| !matches!(param, ast::Param::Pos(_)))
                    {
                        params.insert(params.len().saturating_sub(1), '?');
                    }
                    let body = closure.body();
                    let mut item = supplemental_definition(
                        name.as_str().to_string(),
                        NodeKind::Function,
                        format!("{}{params}", name.as_str()),
                        path,
                        node_lines(&node, content),
                        syntax_text(body.to_untyped()),
                    );
                    item.complexity = 1 + typst_decisions(body.to_untyped());
                    item.is_trivial_wrapper_body = matches!(body, TypstExpr::FuncCall(_));
                    item.has_keep_marker = has_keep_marker(content, item.line_start);
                    result.definitions.push(item);
                }
            }
        }
        SyntaxKind::FuncCall => {
            if let Some(call) = node.get().cast::<ast::FuncCall>() {
                if let Some(name) = typst_callee(call.callee()) {
                    let args: Vec<_> = call.args().items().collect();
                    result.references.push(Reference {
                        name,
                        file_path: path.to_string_lossy().to_string(),
                        line: line_at(content, node.offset()),
                        kind: ReferenceKind::Call,
                        resolved_to: None,
                        call_arity: (!args.iter().any(|arg| matches!(arg, ast::Arg::Spread(_))))
                            .then_some(args.len() as u32),
                    });
                }
            }
        }
        SyntaxKind::ModuleImport => {
            if let Some(import) = node.get().cast::<ast::ModuleImport>() {
                if let TypstExpr::Str(source) = import.source() {
                    let names = match import.imports() {
                        Some(ast::Imports::Wildcard) => vec!["*".into()],
                        Some(ast::Imports::Items(items)) => items
                            .iter()
                            .map(|item| item.bound_name().as_str().to_string())
                            .collect(),
                        None => import
                            .new_name()
                            .map(|name| vec![name.as_str().to_string()])
                            .unwrap_or_default(),
                    };
                    result.imports.push(make_import(
                        path,
                        source.get().as_str(),
                        names,
                        line_at(content, node.offset()),
                    ));
                }
            }
        }
        SyntaxKind::ModuleInclude => {
            if let Some(include) = node.get().cast::<ast::ModuleInclude>() {
                if let TypstExpr::Str(source) = include.source() {
                    result.imports.push(make_import(
                        path,
                        source.get().as_str(),
                        vec!["*".into()],
                        line_at(content, node.offset()),
                    ));
                }
            }
        }
        _ => {}
    }
    for child in node.children() {
        visit_typst(child, path, content, result);
    }
}

fn syntax_text(node: &SyntaxNode) -> String {
    node.clone().into_text().to_string()
}

fn typst_callee(expr: TypstExpr<'_>) -> Option<String> {
    matches!(expr, TypstExpr::Ident(_) | TypstExpr::FieldAccess(_))
        .then(|| syntax_text(expr.to_untyped()))
}

fn typst_decisions(node: &SyntaxNode) -> u32 {
    node.children()
        .map(|child| {
            u32::from(matches!(
                child.kind(),
                SyntaxKind::Conditional | SyntaxKind::WhileLoop | SyntaxKind::ForLoop
            )) + typst_decisions(child)
        })
        .sum()
}

fn node_lines(node: &LinkedNode<'_>, content: &str) -> (u32, u32) {
    let range = node.range();
    (
        line_at(content, range.start),
        line_at(content, range.end.saturating_sub(1)),
    )
}

fn make_import(path: &Path, source: &str, names: Vec<String>, line: u32) -> Import {
    let is_relative = !source.starts_with('@') && !Path::new(source).is_absolute();
    let resolved = if is_relative {
        let joined = path.parent().unwrap_or(Path::new(".")).join(source);
        joined
            .canonicalize()
            .unwrap_or(joined)
            .to_string_lossy()
            .to_string()
    } else {
        source.to_string()
    };
    Import {
        source: resolved,
        imported_names: names,
        file_path: path.to_string_lossy().to_string(),
        line,
        is_relative,
    }
}

fn astro_script_source(content: &str) -> String {
    let mut output = crate::typescript::svelte::extract_script_source(content).into_bytes();
    if let Some((start, end)) = astro_frontmatter(content) {
        output[start..end].copy_from_slice(&content.as_bytes()[start..end]);
    }
    String::from_utf8(output).expect("blanked Astro source remains UTF-8")
}

fn astro_frontmatter(content: &str) -> Option<(usize, usize)> {
    let mut offset = if content.starts_with('\u{feff}') {
        3
    } else {
        0
    };
    let first_end = content[offset..]
        .find('\n')
        .map_or(content.len(), |i| offset + i + 1);
    if content[offset..first_end].trim() != "---" {
        return None;
    }
    let body_start = first_end;
    offset = first_end;
    while offset < content.len() {
        let end = content[offset..]
            .find('\n')
            .map_or(content.len(), |i| offset + i + 1);
        if content[offset..end].trim() == "---" {
            return Some((body_start, offset));
        }
        offset = end;
    }
    None
}

fn astro_template_references(
    content: &str,
    code: &str,
    names: &HashSet<String>,
    file_path: &str,
) -> Vec<Reference> {
    let bytes = content.as_bytes();
    let mask = code.as_bytes();
    let mut references = Vec::new();
    let mut seen = HashSet::new();
    let (mut i, mut line, mut braces) = (0usize, 1u32, 0u32);
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if mask.get(i).is_some_and(|byte| !byte.is_ascii_whitespace()) {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            byte if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'$' | b'-'))
                {
                    i += 1;
                }
                let name = &content[start..i];
                let before = bytes[..start]
                    .iter()
                    .rposition(|byte| !byte.is_ascii_whitespace());
                let component_tag = before.is_some_and(|index| {
                    bytes[index] == b'<'
                        || (bytes[index] == b'/'
                            && bytes[..index]
                                .iter()
                                .rposition(|byte| !byte.is_ascii_whitespace())
                                .is_some_and(|previous| bytes[previous] == b'<'))
                });
                if names.contains(name)
                    && (braces > 0 || component_tag)
                    && seen.insert((name.to_string(), line))
                {
                    references.push(Reference {
                        name: name.to_string(),
                        file_path: file_path.to_string(),
                        line,
                        kind: ReferenceKind::Template,
                        resolved_to: None,
                        call_arity: None,
                    });
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    references
}

fn bash_imports(path: &Path, content: &str) -> Vec<Import> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let mut words = line.split_whitespace();
            let command = words.next()?;
            if command != "source" && command != "." {
                return None;
            }
            let source = words.next()?.trim_matches(['\'', '"', ';']);
            if source.contains('$') || source.is_empty() {
                return None;
            }
            Some(make_import(
                path,
                source,
                vec!["*".into()],
                index as u32 + 1,
            ))
        })
        .collect()
}

fn line_at(content: &str, byte: usize) -> u32 {
    content.as_bytes()[..byte.min(content.len())]
        .iter()
        .filter(|character| **character == b'\n')
        .count() as u32
        + 1
}

fn has_keep_marker(content: &str, line: u32) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    [line.checked_sub(1), line.checked_sub(2)]
        .into_iter()
        .flatten()
        .filter_map(|index| lines.get(index as usize))
        .any(|line| line.contains("keel:keep"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::treesitter::detect_language;

    #[test]
    fn detects_every_supplemental_extension() {
        for (path, language) in [
            ("template.typ", "typst"),
            ("Page.astro", "astro"),
            ("script.sh", "bash"),
            ("migration.sql", "sql"),
        ] {
            assert_eq!(detect_language(Path::new(path)), Some(language));
        }
    }

    #[test]
    fn parses_typst_functions_calls_and_imports() {
        let parsed = parse_typst(
            Path::new("forms/main.typ"),
            "#import \"helpers.typ\": money\n#let total(x, dark: false) = money(x)\n",
        );
        assert_eq!(parsed.definitions[0].signature, "total(x, dark: false?)");
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "money"));
        assert_eq!(parsed.imports[0].imported_names, ["money"]);
    }

    #[test]
    fn parses_astro_frontmatter_and_template_usage() {
        let resolver = SupplementalResolver::new();
        let parsed = resolver.parse_file(
            Path::new("src/Card.astro"),
            "---\nimport Panel from './Panel.astro';\nfunction title(x: string): string { return x; }\n---\n<Panel>{title('x')}</Panel>\n",
        );
        assert!(parsed
            .definitions
            .iter()
            .any(|definition| definition.name == "title"));
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "Panel"));
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "title"));
    }

    #[test]
    fn parses_bash_functions_calls_and_sources() {
        let resolver = SupplementalResolver::new();
        let parsed = resolver.parse_file(
            Path::new("scripts/run.sh"),
            "source ./common.sh\nbuild() { helper \"$1\"; }\nbuild x\n",
        );
        assert!(parsed
            .definitions
            .iter()
            .any(|definition| definition.name == "build"));
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "helper"));
        assert_eq!(parsed.imports.len(), 1);
    }

    #[test]
    fn parses_sql_schema_functions_and_references() {
        let parsed = parse_sql(
            Path::new("migrations/001.sql"),
            "CREATE TABLE accounts(id bigint primary key);\nCREATE TABLE payments(account_id bigint REFERENCES accounts(id));\nCREATE FUNCTION payment_count(x bigint) RETURNS bigint LANGUAGE SQL AS $$ SELECT count(*) FROM payments WHERE account_id = x $$;\nCREATE POLICY payment_read ON payments USING (payment_count(account_id) > 0);\n",
        );
        assert!(parsed
            .definitions
            .iter()
            .any(|definition| definition.name == "accounts"));
        assert!(parsed
            .definitions
            .iter()
            .any(|definition| definition.name == "payment_count"));
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "accounts"));
        assert!(parsed
            .references
            .iter()
            .any(|reference| reference.name == "payments"));
    }

    #[test]
    fn keeps_semicolons_inside_dollar_quoted_sql_bodies() {
        let ranges = sql::sql_statement_ranges(
            "CREATE FUNCTION f() RETURNS void AS $$ BEGIN; END; $$ LANGUAGE plpgsql; SELECT 1;",
        );
        assert_eq!(ranges.len(), 2);
    }
}
