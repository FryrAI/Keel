//! Verification dimension — tests, CI, test coverage, test command documentation.

use std::path::Path;

use crate::types::{AuditFinding, AuditSeverity};

/// File patterns that indicate test files exist.
const TEST_DIR_NAMES: &[&str] = &["tests", "__tests__", "test"];

/// File glob patterns for test files (checked via simple suffix/prefix matching).
const TEST_FILE_PATTERNS: &[(&str, &str)] = &[
    ("test_", ".py"),  // Python: test_*.py
    ("", "_test.go"),  // Go: *_test.go
    ("", "_test.rs"),  // Rust: *_test.rs
    ("", ".test.ts"),  // TS: *.test.ts
    ("", ".test.tsx"), // TSX: *.test.tsx
    ("", ".test.js"),  // JS: *.test.js
    ("", ".test.jsx"), // JSX: *.test.jsx
    ("", ".spec.ts"),  // TS: *.spec.ts
    ("", ".spec.tsx"), // TSX: *.spec.tsx
    ("", ".spec.js"),  // JS: *.spec.js
    ("", ".spec.jsx"), // JSX: *.spec.jsx
];

/// Source file extensions to count.
const SOURCE_EXTENSIONS: &[&str] = &["py", "rs", "ts", "tsx", "js", "jsx", "go"];

/// CI config paths to check.
const CI_CONFIGS: &[&str] = &[
    ".github/workflows",
    ".gitlab-ci.yml",
    "Jenkinsfile",
    ".circleci",
    "bitbucket-pipelines.yml",
];

/// Test command patterns to search for in instruction files.
const TEST_CMD_PATTERNS: &[&str] = &[
    "cargo test",
    "pytest",
    "npm test",
    "go test",
    "make test",
    "yarn test",
    "npx jest",
    "npx vitest",
];

/// Agent instruction files to search for test commands.
const INSTRUCTION_FILES: &[&str] = &[
    "CLAUDE.md",
    ".cursorrules",
    "GEMINI.md",
    "WINDSURF.md",
    "AGENTS.md",
    "COPILOT.md",
    "README.md",
];

pub fn check_verification(root_dir: &Path) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    let (test_file_count, source_file_count) = count_test_and_source_files(root_dir);
    let has_test_dir = TEST_DIR_NAMES.iter().any(|d| root_dir.join(d).is_dir());
    let has_tests = has_test_dir || test_file_count > 0;

    // Check 1: has_tests
    if !has_tests {
        findings.push(AuditFinding {
            severity: AuditSeverity::Fail,
            check: "has_tests".into(),
            message: "No test files found".into(),
            tip: Some(
                "Create a tests/ directory with at least one test. For Rust: add \
                 #[cfg(test)] mod tests in src/ or create tests/. For Python: create \
                 tests/test_<module>.py. Then verify with your test runner."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    // Check 2: test_command_documented
    let test_cmd_documented = INSTRUCTION_FILES.iter().any(|f| {
        let path = root_dir.join(f);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let lower = content.to_lowercase();
            TEST_CMD_PATTERNS.iter().any(|pat| lower.contains(pat))
        } else {
            false
        }
    });

    if !test_cmd_documented {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "test_command_documented".into(),
            message: "No test command found in agent instruction files or README".into(),
            tip: Some(
                "Add a ## Testing section to CLAUDE.md with the exact command:\n  \
                 ```bash\n  cargo test\n  ```\n\
                 Agents need to know how to verify their changes."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    // Check 3: test_coverage_ratio
    if has_tests && source_file_count > 0 {
        let ratio = test_file_count as f64 / source_file_count as f64;
        if ratio < 0.1 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "test_coverage_ratio".into(),
                message: format!(
                    "Low test coverage: {} test files for {} source files ({:.0}%)",
                    test_file_count,
                    source_file_count,
                    ratio * 100.0,
                ),
                tip: Some(format!(
                    "Low test coverage: {} test files for {} source files. Run \
                     `keel map --llm` to identify high-caller functions — these are \
                     the highest-value targets for new tests.",
                    test_file_count, source_file_count,
                )),
                file: None,
                count: Some(test_file_count as u32),
            });
        }
    }

    // Check 4: has_ci_config
    let has_ci = CI_CONFIGS.iter().any(|c| root_dir.join(c).exists());
    if !has_ci {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "has_ci_config".into(),
            message: "No CI configuration found".into(),
            tip: Some(
                "Add CI configuration to run tests on every push. Create \
                 .github/workflows/ci.yml with steps for checkout, build, and test."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    // Check 5: has_lint_config
    let lint_configs: &[&str] = &[
        "clippy.toml",
        ".clippy.toml",
        ".eslintrc.json",
        ".eslintrc.js",
        ".eslintrc.yml",
        ".eslintrc.yaml",
        "eslint.config.js",
        "eslint.config.mjs",
        "eslint.config.ts",
        "ruff.toml",
        "tsconfig.json",
        ".golangci.yml",
        ".golangci.yaml",
        "biome.json",
    ];
    let has_lint_via_file = lint_configs.iter().any(|c| root_dir.join(c).exists());
    // Also check pyproject.toml for [tool.ruff] or [tool.mypy]
    let has_lint_via_pyproject = root_dir.join("pyproject.toml").exists() && {
        std::fs::read_to_string(root_dir.join("pyproject.toml"))
            .map(|c| c.contains("[tool.ruff]") || c.contains("[tool.mypy]"))
            .unwrap_or(false)
    };
    if !has_lint_via_file && !has_lint_via_pyproject {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "has_lint_config".into(),
            message: "No linter or type-checker configuration found".into(),
            tip: Some(
                "Configure a linter as a guardrail. For Rust: clippy is built-in (add \
                 clippy.toml for customization). For Python: add ruff.toml or [tool.ruff] \
                 to pyproject.toml. For TypeScript: ensure tsconfig.json exists with strict \
                 mode. For Go: add .golangci.yml."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    findings
}

/// Count test files and source files by walking the directory tree.
fn count_test_and_source_files(root_dir: &Path) -> (usize, usize) {
    let mut test_count = 0usize;
    let mut source_count = 0usize;

    let walker = walkdir::WalkDir::new(root_dir)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            // Skip hidden dirs (except .github), node_modules, target, vendor
            if e.file_type().is_dir() {
                return !matches!(
                    name,
                    "node_modules" | "target" | "vendor" | ".git" | "dist" | "build"
                );
            }
            true
        });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_str().unwrap_or("");
        let path_str = entry.path().to_str().unwrap_or("");
        let is_test = is_test_file(name) || is_in_test_dir(path_str);
        let is_source = SOURCE_EXTENSIONS
            .iter()
            .any(|ext| name.ends_with(&format!(".{}", ext)));

        // For Rust files, also check for inline #[cfg(test)] modules
        if !is_test && name.ends_with(".rs") {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if content.contains("#[cfg(test)]") {
                    test_count += 1;
                    // Still count as source too (it's both)
                    source_count += 1;
                    continue;
                }
            }
        }

        if is_test {
            test_count += 1;
        } else if is_source {
            source_count += 1;
        }
    }

    (test_count, source_count)
}

/// Check if a file is inside a test directory (tests/, __tests__/, etc.).
fn is_in_test_dir(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.contains("/tests/")
        || normalized.contains("/__tests__/")
        || normalized.contains("/test/")
}

/// Check if a filename matches test file patterns.
fn is_test_file(name: &str) -> bool {
    // Rust inline tests (#[cfg(test)]) won't show as separate files,
    // but *_test.rs files in tests/ will.
    for (prefix, suffix) in TEST_FILE_PATTERNS {
        if !prefix.is_empty() && !suffix.is_empty() {
            if name.starts_with(prefix) && name.ends_with(suffix) {
                return true;
            }
        } else if !prefix.is_empty() {
            if name.starts_with(prefix) {
                return true;
            }
        } else if !suffix.is_empty() && name.ends_with(suffix) {
            return true;
        }
    }
    false
}
