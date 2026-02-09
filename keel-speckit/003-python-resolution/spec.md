# Spec 003: Python Resolution — ty-Powered Tier 2

```yaml
tags: [keel, spec, python, ty, tier-2, resolution]
owner: Agent A (Foundation)
dependencies: [Spec 000 (graph schema), Spec 001 (tree-sitter foundation)]
prd_sections: [10.1]
priority: P1 — Python is a primary target language alongside TypeScript
```

## Summary

This spec defines Tier 2 resolution for Python via the ty type checker as a subprocess. ty (formerly red-knot, from the ruff team) provides cross-file type checking and resolution via `ty --output-format json`. It is beta (v0.0.15), with multiple releases per week, built on the Salsa incremental computation framework achieving 4.7ms incremental updates. Because ty's crates live inside the ruff monorepo and are not published to crates.io, direct Rust library integration is impractical for Phase 1 — subprocess invocation is the correct approach. When ty is not installed, keel degrades gracefully to tree-sitter heuristics with optional Pyright LSP fallback. This targets ~82-99% resolution precision depending on ty availability.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10.1 Tier 2 Python | ty subprocess for cross-file resolution, fallback to tree-sitter heuristics |
| 10.1 Tier 2 General | Tier 2 resolvers run per-language, fill edges the tree-sitter layer cannot resolve |
| 10.1 Precision | Combined Tier 1 + Tier 2 target: ~82-99% for Python (ty-dependent) |

---

## Technical Specification

### Scope

**File types:** `.py`, `.pyi` (stub files)

**Resolution capabilities:**
- Absolute imports (`from package.module import func`)
- Relative imports (`from . import sibling`, `from ..parent import func`)
- `__init__.py` package resolution
- Star imports (`from module import *`) — flagged as ambiguous
- Type annotation imports (`from typing import ...`, `from __future__ import annotations`)
- Conditional imports (`if TYPE_CHECKING:` blocks)
- Re-exports via `__all__`

**Out of scope (deferred to Tier 3 LSP):**
- Dynamic imports (`importlib.import_module()`)
- Monkey-patching resolution
- Plugin/entry-point resolution (`setuptools` / `pyproject.toml`)
- Runtime-computed attribute access (`getattr()`)

### ty Subprocess Integration

#### Why Subprocess, Not Library

ty's crates (`red_knot_python_semantic`, `red_knot_module_resolver`, etc.) live inside the ruff monorepo and are NOT published to crates.io. They are internal crates with unstable APIs. Direct Rust library integration would require:
- Vendoring the entire ruff monorepo as a dependency
- Tracking internal API changes across multiple releases per week
- Managing Salsa framework initialization that ty handles internally

**Decision:** Subprocess invocation via `ty --output-format json` is the correct Phase 1 approach. Library integration is deferred to Phase 2 when ty stabilizes and potentially publishes standalone crates.

#### ty Subprocess Interface

```rust
use std::process::Command;
use serde::Deserialize;

pub struct TyResolver {
    ty_binary: PathBuf,
    project_root: PathBuf,
    available: bool,
}

impl TyResolver {
    /// Probe for ty installation. If not found, mark as unavailable.
    pub fn new(project_root: &Path) -> Self {
        let ty_binary = Self::find_ty_binary();
        let available = ty_binary.is_some();
        if !available {
            tracing::warn!(
                "ty not found. Python resolution will use tree-sitter \
                 heuristics only. Install ty: `pip install ty`"
            );
        }
        Self {
            ty_binary: ty_binary.unwrap_or_default(),
            project_root: project_root.to_path_buf(),
            available,
        }
    }

    /// Run ty on the project and parse structured output.
    pub fn resolve_project(&self) -> Result<TyOutput, TyError> {
        if !self.available {
            return Err(TyError::NotInstalled);
        }

        let output = Command::new(&self.ty_binary)
            .arg("check")
            .arg("--output-format")
            .arg("json")
            .arg("--project")
            .arg(&self.project_root)
            .output()
            .map_err(TyError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TyError::CheckFailed(stderr.to_string()));
        }

        let ty_output: TyOutput = serde_json::from_slice(&output.stdout)
            .map_err(TyError::ParseFailed)?;

        Ok(ty_output)
    }

    /// Check ty version. Minimum supported: v0.0.15.
    fn find_ty_binary() -> Option<PathBuf> {
        let output = Command::new("ty")
            .arg("--version")
            .output()
            .ok()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            if Self::version_meets_minimum(&version, 0, 0, 15) {
                return Some(PathBuf::from("ty"));
            }
        }
        None
    }
}

#[derive(Debug, Deserialize)]
pub struct TyOutput {
    pub diagnostics: Vec<TyDiagnostic>,
    pub type_info: Vec<TyTypeInfo>,
}

#[derive(Debug, Deserialize)]
pub struct TyDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TyTypeInfo {
    pub file: String,
    pub symbol: String,
    pub resolved_from: Option<String>,  // source module path
    pub line: u32,
}
```

#### ty Performance Characteristics

- **Full project check:** Depends on project size. FastAPI (~50k LOC): ~2-3s
- **Incremental update:** 4.7ms average (Salsa framework)
- **Memory:** ~200MB for medium projects
- **Subprocess overhead:** ~50ms startup per invocation

For keel's use case, ty is invoked once per `keel map` / `keel compile` cycle, not per-file. The subprocess overhead is negligible relative to the full analysis.

### Fallback: Tree-sitter Heuristics

When ty is not installed, Python resolution falls back to tree-sitter heuristics from [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]:

```rust
pub struct PythonHeuristicResolver {
    project_root: PathBuf,
    init_packages: HashSet<PathBuf>,  // directories with __init__.py
}

impl PythonHeuristicResolver {
    /// Resolve a Python import using file-system heuristics.
    pub fn resolve_import(
        &self,
        source_file: &Path,
        import_path: &str,
        is_relative: bool,
    ) -> Result<HeuristicResolution, ResolveError> {
        if is_relative {
            self.resolve_relative(source_file, import_path)
        } else {
            self.resolve_absolute(import_path)
        }
    }

    /// Resolve relative import by walking parent directories.
    fn resolve_relative(
        &self,
        source_file: &Path,
        import_path: &str,
    ) -> Result<HeuristicResolution, ResolveError> {
        let dots = import_path.chars().take_while(|c| *c == '.').count();
        let module_part = &import_path[dots..];
        let mut base = source_file.parent().unwrap().to_path_buf();
        for _ in 1..dots {
            base = base.parent().unwrap().to_path_buf();
        }
        self.find_module(&base, module_part)
    }

    /// Resolve absolute import by searching from project root.
    fn resolve_absolute(
        &self,
        import_path: &str,
    ) -> Result<HeuristicResolution, ResolveError> {
        self.find_module(&self.project_root, import_path)
    }

    /// Scan for __init__.py files to identify Python packages.
    fn discover_packages(root: &Path) -> HashSet<PathBuf> {
        // Walk directory tree, collect dirs containing __init__.py
        todo!()
    }
}

#[derive(Debug)]
pub struct HeuristicResolution {
    pub resolved_path: PathBuf,
    pub confidence: f64,
    pub is_heuristic: bool,  // always true for this resolver
}
```

### Optional Pyright LSP Fallback

When ty is unavailable and higher precision is desired, keel can optionally use Pyright as a Tier 3 LSP oracle:

```rust
pub struct PyrightFallback {
    available: bool,
}

impl PyrightFallback {
    /// Probe for pyright-langserver.
    pub fn new() -> Self {
        let available = Command::new("pyright-langserver")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Self { available }
    }
}
```

This is a lightweight fallback — full LSP integration is Tier 3 and covered in a separate spec.

### Python-Specific Resolution Details

#### Relative Imports

```python
# src/auth/login.py
from . import utils          # resolves to src/auth/utils.py
from ..models import User    # resolves to src/models.py::User
from .helpers import hash    # resolves to src/auth/helpers.py::hash
```

**Strategy:** Count leading dots, walk up directories, then resolve remaining path components. Requires `__init__.py` at each package level.

#### `__init__.py` Package Resolution

```python
# src/auth/__init__.py
from .login import authenticate
from .register import create_user
```

**Strategy:**
1. Discover all `__init__.py` files during project scan
2. When `from auth import authenticate` is encountered, check `auth/__init__.py` for re-exports
3. Trace through to the actual definition file (similar to TypeScript barrel files)
4. `__init__.py` with no explicit exports: all public names from the package are importable

#### Star Imports

```python
from utils import *  # imports all public names from utils
```

**Strategy:**
- If `utils` defines `__all__`, use that as the export list
- If no `__all__`, import all names not starting with `_`
- **Always flag star imports as ambiguous** with confidence <= 0.60
- Emit `WARNING: star import from 'utils' — resolution is ambiguous`

#### Type Annotations for Enforcement

```python
def create_user(name: str, email: str) -> User:
    ...
```

**Strategy:**
- Extract type annotations from function signatures
- Set `type_hints_present: true` on `GraphNode` when all parameters and return type are annotated
- Type imports from `typing` module are tracked but don't generate `Calls` edges
- `if TYPE_CHECKING:` blocks: imports inside are type-only (similar to TS `import type`)

### Integration with Graph Schema

Tier 2 Python resolution produces:
- `GraphEdge { kind: EdgeKind::Imports }` for import statements
- `GraphEdge { kind: EdgeKind::Calls }` for resolved cross-file function calls
- `GraphEdge { kind: EdgeKind::Inherits }` for class inheritance across files
- Resolution results cached in `resolution_cache` table with `resolution_tier: "tier2_py"` or `"tier2_py_heuristic"`

### Confidence Scoring

| Resolution Path | Confidence |
|----------------|------------|
| ty-resolved cross-file import | 0.95 |
| ty-resolved type annotation | 0.93 |
| Heuristic: relative import with `__init__.py` present | 0.85 |
| Heuristic: absolute import, single match on filesystem | 0.80 |
| Heuristic: `__init__.py` re-export traced to definition | 0.78 |
| Star import with `__all__` defined | 0.65 |
| Star import without `__all__` | 0.50 |
| Heuristic: absolute import, multiple candidates | 0.40 |

---

## Acceptance Criteria

**GIVEN** a Python project with relative imports (`from ..models import User`)
**WHEN** ty is installed and resolution runs
**THEN** the `Imports` edge resolves to the correct file and symbol with confidence >= 0.93

**GIVEN** a Python package with `__init__.py` re-exporting symbols from submodules
**WHEN** a consumer imports from the package name
**THEN** the resolution traces through `__init__.py` to the actual definition file

**GIVEN** a file with `from module import *`
**WHEN** resolution runs
**THEN** the edge is created with confidence <= 0.60 and a `WARNING: star import` annotation

**GIVEN** a function with full type annotations (`def f(x: int) -> str`)
**WHEN** the function node is created
**THEN** `type_hints_present` is set to `true` on the `GraphNode`

**GIVEN** ty is NOT installed on the system
**WHEN** `TyResolver::new()` is called
**THEN** it logs a warning and `available` is set to `false`, and resolution falls back to tree-sitter heuristics

**GIVEN** ty is installed and a project has a `if TYPE_CHECKING:` import block
**WHEN** resolution runs
**THEN** imports inside the block are marked as `type_only: true`

**GIVEN** a Python project with nested packages (`a/b/c/__init__.py`)
**WHEN** `from a.b.c import func` is resolved
**THEN** each package level has a valid `__init__.py` and the resolution walks the full chain

**GIVEN** the heuristic resolver encounters an absolute import matching two candidate files
**WHEN** resolution runs without ty
**THEN** the edge is created with confidence 0.40 and both candidates are noted in metadata

---

## Test Strategy

**Oracle:** LSP ground truth (Oracle 1 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])
- Run Pyright on test repos
- Capture "Go to Definition" results for all imports and cross-file calls
- Compare keel Tier 2 resolution (both ty and heuristic modes) against LSP results
- Measure precision as: correct edges / total edges produced

**Test repositories:**
- **FastAPI** — heavy use of type annotations, relative imports, `__init__.py` re-exports
- **httpx** — complex package structure, conditional imports, `__all__` usage
- **django-ninja** — decorators, class-based views, inheritance across files

**Test files to create:**
- `tests/py_resolution/test_relative_imports.rs` (~6 tests)
- `tests/py_resolution/test_init_packages.rs` (~6 tests)
- `tests/py_resolution/test_star_imports.rs` (~4 tests)
- `tests/py_resolution/test_type_hints.rs` (~5 tests)
- `tests/py_resolution/test_ty_subprocess.rs` (~5 tests)
- `tests/py_resolution/test_heuristic_fallback.rs` (~5 tests)
- `tests/py_resolution/test_confidence_scoring.rs` (~4 tests)

**Estimated test count:** ~35

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| ty is beta (v0.0.15) — output format may change between releases | Pin minimum version. Parse JSON output defensively with `serde(default)`. |
| ty subprocess startup adds latency | Invoked once per `keel map` cycle, not per-file. 50ms startup is negligible. |
| ty not installed on developer machines | Graceful fallback to heuristics. Clear warning message with install instructions. |
| Star imports in large codebases cause combinatorial blowup | Cap `import *` resolution to 50 symbols per import. Flag as low confidence. |
| Virtual environments / `sys.path` manipulation | ty respects `pyproject.toml` and virtual environments. Heuristic resolver does not — it searches project root only. |
| `__init__.py`-less namespace packages (PEP 420) | Supported by ty. Heuristic resolver treats directories without `__init__.py` as non-packages. Document this limitation. |

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> Agent B):

**`TyResolver` struct:**
- `new(project_root: &Path) -> Self` — initialize resolver, probe for ty
- `resolve_project() -> Result<TyOutput, TyError>` — run ty on full project
- `available: bool` — whether ty is installed

**`PythonHeuristicResolver` struct:**
- `new(project_root: &Path) -> Self` — initialize heuristic resolver
- `resolve_import(source: &Path, import_path: &str, is_relative: bool) -> Result<HeuristicResolution, ResolveError>`

**Edge production:**
- All resolved edges conform to `GraphEdge` from [[keel-speckit/000-graph-schema/spec|Spec 000]]
- Resolution results written to `resolution_cache` table with `resolution_tier: "tier2_py"` or `"tier2_py_heuristic"`
- Confidence scores attached to every produced edge

### Consumed from other specs:

**From [[keel-speckit/000-graph-schema/spec|Spec 000]]:**
- `GraphNode`, `GraphEdge`, `EdgeKind` types
- `GraphStore` trait for reading/writing graph
- `resolution_cache` table schema

**From [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]:**
- Tree-sitter parsed ASTs for heuristic fallback
- Call-site extraction (function calls that need cross-file resolution)
- File-level module node creation
- `__init__.py` detection during file scanning

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures this resolver populates
- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — provides ASTs and heuristic fallback
- [[keel-speckit/002-typescript-resolution/spec|Spec 002: TypeScript Resolution]] — sibling Tier 2 resolver (Oxc toolchain)
- [[keel-speckit/004-go-resolution/spec|Spec 004: Go Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/005-rust-resolution/spec|Spec 005: Rust Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — consumes edges produced here
