//! tsconfig.json discovery, comment stripping, `extends` chain following, and
//! path-alias merging.
//!
//! tsconfig files are JSONC (JSON with comments and, in practice, trailing
//! commas). `serde_json` rejects both, so [`strip_jsonc_comments`] normalizes
//! the text first.
//!
//! Alias resolution semantics follow TypeScript, with one deliberate
//! narrowing:
//! - `compilerOptions.paths` entries are resolved against `compilerOptions.baseUrl`,
//!   which is itself relative to the directory of the tsconfig that declares it.
//! - A tsconfig that `extends` another inherits its `paths`; entries declared by
//!   the child win over the parent.
//! - Only the FIRST target of each `paths` array is used. TypeScript tries
//!   every entry and falls through on missing files; keel does not (yet), so
//!   fallback arrays (`["src/app/*", "generated/app/*"]`) resolve only via
//!   their first entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Maximum number of `extends` hops followed before giving up.
///
/// Guards against cyclic `extends` chains without needing a visited set.
const MAX_EXTENDS_DEPTH: usize = 8;

/// SvelteKit's virtual module prefixes. These are provided by the framework at
/// build time and never correspond to a file in the repo, so they are treated
/// as external rather than left as unresolved imports.
const SVELTEKIT_FRAMEWORK_PREFIXES: &[&str] = &["$app", "$env", "$service-worker"];

/// Returns true if `source` is a SvelteKit framework-provided virtual module
/// (`$app/*`, `$env/*`, `$service-worker`).
///
/// These specifiers are intentionally never resolved to a file on disk.
pub(crate) fn is_sveltekit_framework_module(source: &str) -> bool {
    // Zero-alloc: this runs for every import of every TS-family file.
    SVELTEKIT_FRAMEWORK_PREFIXES.iter().any(|p| {
        source
            .strip_prefix(p)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Removes `//` line comments and `/* */` block comments from JSONC text.
///
/// String literals are respected: comment markers inside a quoted string (and
/// escaped quotes within it) are left untouched. Removed characters other than
/// newlines are dropped, so byte offsets are not preserved — this output is only
/// fed to `serde_json`.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev_star = false;
                for c in chars.by_ref() {
                    if c == '\n' {
                        // Preserve newlines so JSON error line numbers stay sane.
                        out.push('\n');
                    }
                    if prev_star && c == '/' {
                        break;
                    }
                    prev_star = c == '*';
                }
            }
            _ => out.push(c),
        }
    }

    out
}

/// Removes trailing commas before `}` or `]`, which tsconfig files commonly
/// contain but `serde_json` rejects.
fn strip_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_comma: Option<usize> = None;

    for c in input.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                pending_comma = None;
                in_string = true;
                out.push(c);
            }
            ',' => {
                pending_comma = Some(out.len());
                out.push(c);
            }
            '}' | ']' => {
                if let Some(idx) = pending_comma.take() {
                    out.remove(idx);
                }
                out.push(c);
            }
            c if c.is_whitespace() => out.push(c),
            _ => {
                pending_comma = None;
                out.push(c);
            }
        }
    }

    out
}

/// Parses a tsconfig-style JSONC file from disk.
fn read_jsonc(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    let cleaned = strip_trailing_commas(&strip_jsonc_comments(&content));
    serde_json::from_str(&cleaned).ok()
}

/// Resolves the target of an `"extends"` value to a tsconfig file path.
///
/// Relative targets (`./base.json`, `../tsconfig.base.json`) resolve against
/// `from_dir`. Bare targets are looked up in `node_modules`, walking upward from
/// `from_dir`. A `.json` extension is appended when the target has none.
fn resolve_extends_target(from_dir: &Path, target: &str) -> Option<PathBuf> {
    let with_ext = |p: PathBuf| -> PathBuf {
        if p.extension().is_some() {
            p
        } else {
            p.with_extension("json")
        }
    };

    if target.starts_with('.') || target.starts_with('/') {
        let direct = with_ext(from_dir.join(target));
        if direct.is_file() {
            return Some(direct);
        }
        // `extends: "./config"` may name a directory containing tsconfig.json.
        let as_dir = from_dir.join(target).join("tsconfig.json");
        return as_dir.is_file().then_some(as_dir);
    }

    let mut dir = Some(from_dir);
    while let Some(d) = dir {
        let base = d.join("node_modules").join(target);
        let direct = with_ext(base.clone());
        if direct.is_file() {
            return Some(direct);
        }
        let as_dir = base.join("tsconfig.json");
        if as_dir.is_file() {
            return Some(as_dir);
        }
        dir = d.parent();
    }
    None
}

/// Merges the `compilerOptions.paths` of a single tsconfig into `out`.
///
/// Targets are made absolute against `baseUrl` (default `.`), itself relative to
/// `config_dir`. Existing entries in `out` are overwritten, so callers must visit
/// parents before children.
fn merge_paths(json: &serde_json::Value, config_dir: &Path, out: &mut HashMap<String, String>) {
    let Some(compiler_options) = json.get("compilerOptions") else {
        return;
    };
    let Some(paths) = compiler_options.get("paths").and_then(|p| p.as_object()) else {
        return;
    };
    let base_url = compiler_options
        .get("baseUrl")
        .and_then(|b| b.as_str())
        .unwrap_or(".");
    let base = config_dir.join(base_url);

    for (alias, targets) in paths {
        let Some(target) = targets
            .as_array()
            .and_then(|a| a.first())
            .and_then(|t| t.as_str())
        else {
            continue;
        };
        let clean_alias = alias.trim_end_matches("/*").to_string();
        let clean_target = target.trim_end_matches("/*");
        let resolved = normalize(&base.join(clean_target))
            .to_string_lossy()
            .to_string();
        out.insert(clean_alias, resolved);
    }
}

/// Lexically removes `.` and `..` components from a path.
///
/// Used instead of `canonicalize` because tsconfig targets frequently point at
/// directories that do not exist yet (generated output, unbuilt packages).
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            // A `..` may only cancel a preceding NORMAL component. Popping
            // blindly would also cancel an earlier retained `..` (so
            // `../../shared` became `shared`), and popping at the root must
            // not push `..` above it (`/..` matches nothing).
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Walks the `extends` chain of `config_path` and merges every `paths` block,
/// deepest ancestor first so that nearer configs win.
fn collect_from_config(config_path: &Path, depth: usize, out: &mut HashMap<String, String>) {
    if depth > MAX_EXTENDS_DEPTH {
        return;
    }
    let Some(json) = read_jsonc(config_path) else {
        return;
    };
    let config_dir = config_path.parent().unwrap_or(Path::new("."));

    if let Some(extends) = json.get("extends") {
        // TypeScript 5 allows an array of parents, applied left to right.
        let targets: Vec<&str> = match extends {
            serde_json::Value::String(s) => vec![s.as_str()],
            serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
            _ => vec![],
        };
        for target in targets {
            if let Some(parent) = resolve_extends_target(config_dir, target) {
                collect_from_config(&parent, depth + 1, out);
            }
        }
    }

    merge_paths(&json, config_dir, out);

    // Project references are sibling projects, not ancestors: pull in their
    // aliases without letting them override the root project's.
    if depth == 0 {
        if let Some(refs) = json.get("references").and_then(|r| r.as_array()) {
            for reference in refs {
                let Some(ref_path) = reference.get("path").and_then(|p| p.as_str()) else {
                    continue;
                };
                let ref_root = config_dir.join(ref_path);
                let ref_config = if ref_root.is_dir() {
                    ref_root.join("tsconfig.json")
                } else {
                    ref_root
                };
                if ref_config.is_file() {
                    let mut ref_aliases = HashMap::new();
                    collect_from_config(&ref_config, depth + 1, &mut ref_aliases);
                    for (alias, target) in ref_aliases {
                        out.entry(alias).or_insert(target);
                    }
                }
            }
        }
    }
}

/// Loads all path aliases visible from `project_root`.
///
/// Reads `<project_root>/tsconfig.json` (falling back to `jsconfig.json`),
/// follows its `extends` chain and project `references`, and merges every
/// `compilerOptions.paths` block into absolute target directories.
///
/// When the project is a SvelteKit app (a `svelte.config.{js,ts,mjs}` exists)
/// and `$lib` was not supplied by any tsconfig — typically because
/// `.svelte-kit/tsconfig.json` has not been generated yet — the SvelteKit
/// default `$lib -> <project_root>/src/lib` is registered.
pub(crate) fn load_aliases(project_root: &Path) -> HashMap<String, String> {
    let mut aliases = HashMap::new();

    for name in ["tsconfig.json", "jsconfig.json"] {
        let candidate = project_root.join(name);
        if candidate.is_file() {
            collect_from_config(&candidate, 0, &mut aliases);
            break;
        }
    }

    if is_sveltekit_project(project_root) && !aliases.contains_key("$lib") {
        let lib = normalize(&project_root.join("src").join("lib"));
        aliases.insert("$lib".to_string(), lib.to_string_lossy().to_string());
    }

    aliases
}

/// Returns true if `root` contains a SvelteKit config file.
fn is_sveltekit_project(root: &Path) -> bool {
    ["svelte.config.js", "svelte.config.ts", "svelte.config.mjs"]
        .iter()
        .any(|f| root.join(f).is_file())
}

#[cfg(test)]
#[path = "tsconfig_tests.rs"]
mod tests;
