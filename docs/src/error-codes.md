# Error Codes

Every keel violation includes an error code, a human-readable message, a `fix_hint` with actionable remediation, a `confidence` score (0.0-1.0), and a `resolution_tier` indicating how the edge was resolved.

## Errors

Errors indicate structural problems that must be fixed. They cause `keel compile` to exit with code `1`.

### E001 — Broken Caller

**Severity:** ERROR

A function calls another function that no longer exists or has been renamed.

```json
{
  "code": "E001",
  "message": "broken caller: login() calls authenticate() which no longer exists",
  "file": "src/auth.ts",
  "line": 42,
  "hash": "a7Bx3kM9f2Q",
  "fix_hint": "Update login() to use the new verifyCredentials() function",
  "confidence": 0.95,
  "resolution_tier": "tree-sitter"
}
```

**Common causes:**
- Renaming a function without updating callers
- Deleting a function that other code depends on
- Moving a function to a different module

**Fix:** Update the caller to reference the new function name, or restore the deleted function. Use `keel discover <hash>` to see all callers of the affected symbol.

### E002 — Missing Type Hints

**Severity:** ERROR

A function is missing type annotations. Applies primarily to Python and JavaScript.

```json
{
  "code": "E002",
  "message": "missing type hints: process_data() has no type annotations",
  "file": "src/pipeline.py",
  "line": 15,
  "hash": "b3Kx9mN2f4R",
  "fix_hint": "Add type annotations: def process_data(items: list[str]) -> dict[str, int]"
}
```

**Language behavior:**
- **Python:** Requires explicit `def f(x: int) -> str` annotations
- **JavaScript:** Requires JSDoc `@param` and `@returns` tags
- **TypeScript, Go, Rust:** Already statically typed — validates signatures match callers

**Fix:** Add type annotations to the function signature. Disable with `enforce.type_hints: false` in `keel.json`, or suppress per-run with `--suppress E002`.

### E003 — Missing Docstring

**Severity:** ERROR

A public function lacks documentation.

```json
{
  "code": "E003",
  "message": "missing docstring: UserService.create_user() has no documentation",
  "file": "src/services/user.py",
  "line": 28,
  "hash": "c5Mx2kP8f1Q",
  "fix_hint": "Add a docstring explaining the function's purpose and parameters"
}
```

**Fix:** Add a docstring or documentation comment. Disable with `enforce.docstrings: false` in `keel.json`.

### E004 — Function Removed

**Severity:** ERROR

A function that previously existed in the graph has been removed, and other code still references it.

```json
{
  "code": "E004",
  "message": "function removed: validateEmail() was deleted but has 3 callers",
  "file": "src/validation.ts",
  "line": 0,
  "hash": "d8Nx5kR3f7Q",
  "fix_hint": "Restore validateEmail() or update callers: signup(), updateProfile(), importUsers()"
}
```

**Fix:** Either restore the function or update all callers. Use `keel discover <hash>` to find all callers before removing a function.

### E005 — Arity Mismatch

**Severity:** ERROR

A function is called with the wrong number of arguments.

```json
{
  "code": "E005",
  "message": "arity mismatch: createUser() expects 3 args but login() passes 2",
  "file": "src/auth.ts",
  "line": 55,
  "hash": "e2Px8kS4f9Q",
  "fix_hint": "createUser() signature is (name: string, email: string, role: string) — add the missing 'role' argument"
}
```

**Fix:** Update the call site to pass the correct number of arguments, or update the function signature to match the intended usage.

### E006 — Layer Violation

**Severity:** ERROR (opt-in only)

A cross-boundary dependency matches an ordered pair in `architecture.deny`. This is the escalation of [W009](#w009--new-cross-boundary-dependency); with no deny list configured — the default — E006 can never fire.

```json
{
  "code": "E006",
  "message": "`harness` must not depend on `core` (denied in architecture.deny) — calls `raster_ingest`",
  "file": "crates/harness/src/run.rs",
  "line": 12,
  "hash": "i3Tx7kW9f0Q",
  "fix_hint": "`core` already exposes `execute` — go through it instead of reaching into `crates/core/src/ingest.rs`, or move the shared code into a boundary both already depend on",
  "confidence": 0.9,
  "resolution_tier": "heuristic"
}
```

Declare the denied pairs in `.keel/keel.json`:

```json
{
  "architecture": {
    "deny": [["harness", "core"], ["core", "frontend"]]
  }
}
```

Pairs are ordered: `["harness", "core"]` denies `harness → core` and says nothing about `core → harness`.

**Fix:** Route through the target boundary's public surface, or move the shared code into a boundary both sides already depend on.

## Warnings

Warnings indicate potential issues that don't block compilation. They cause exit code `0` in normal mode, or exit code `1` with `--strict`.

### W001 — Placement

**Severity:** WARNING

A function is defined in a module where it doesn't structurally belong based on its dependencies and naming.

```json
{
  "code": "W001",
  "message": "placement: sendEmail() in src/auth.ts has no callers or callees in this module",
  "file": "src/auth.ts",
  "line": 120,
  "hash": "f4Qx1kT6f3Q",
  "fix_hint": "Consider moving sendEmail() to src/notifications.ts (3 callers there)"
}
```

**Fix:** Move the function to the suggested module, or suppress with `enforce.placement: false` in `keel.json`.

### W002 — Duplicate Name

**Severity:** WARNING

Multiple symbols share the same name in overlapping scope.

```json
{
  "code": "W002",
  "message": "duplicate name: validate() defined in src/auth.ts:10 and src/forms.ts:25",
  "file": "src/auth.ts",
  "line": 10,
  "hash": "g6Rx3kU8f5Q",
  "fix_hint": "Rename to validateAuth() or validateCredentials() to distinguish from forms.validate()"
}
```

**Fix:** Rename one of the symbols to be more specific. Use `keel name "<description>"` for naming suggestions.

Two files that Cargo compiles as *separate* units — `build.rs`, `src/main.rs`, each `src/bin/*.rs`, each `examples/*.rs` — never collide with each other, so a name shared between them is exempt whatever the name is. A pair involving a library file still fires.

### W005 — Dead Code

**Severity:** WARNING

A private function has no callers anywhere in the graph and isn't referenced in the current compile batch.

```json
{
  "code": "W005",
  "message": "Function `computeLegacyDiscount` has no callers",
  "file": "src/pricing.ts",
  "line": 88,
  "hash": "h1Sx4kV2f6Q",
  "fix_hint": "No callers found for `computeLegacyDiscount` — delete it, or wire it in and re-run `keel map` to refresh call edges",
  "confidence": 0.7,
  "resolution_tier": "heuristic"
}
```

**Fix:** Delete the function, or call it from somewhere and re-run `keel map` to refresh call edges. Public functions, entrypoints, tests, decorated functions (e.g. `@app.route`, `@pytest.fixture`), and underscore-prefixed names are exempt. For a function reached only through dynamic dispatch the graph can't see (`globals()[name]()`, `getattr`, a string-keyed handler table), add a `keel:keep` marker comment on its definition line or the line above (`# keel:keep`, `// keel:keep`) to suppress it individually. Disable the whole check with `enforce.dead_code: false` in `keel.json`.

### W006 — Duplicate Implementation

**Severity:** WARNING

A function body duplicates one already defined in another file, in one of two tiers:

- **Type-1** — identical after whitespace normalization. Confidence 0.85.
- **Type-2** — identical in structure once identifiers are renamed and literals collapsed, i.e. a copy-paste-then-rename. Confidence 0.6, and only reported when Type-1 found nothing.

```json
{
  "code": "W006",
  "message": "Body of `formatCurrency` is identical to `formatMoney` at src/utils/money.ts:12",
  "file": "src/checkout/format.ts",
  "line": 30,
  "hash": "",
  "fix_hint": "Call `formatMoney` (src/utils/money.ts:12) instead of duplicating it, or extract a shared helper",
  "confidence": 0.85,
  "resolution_tier": "heuristic"
}
```

A Type-2 finding reads:

```json
{
  "code": "W006",
  "message": "Body of `formatCurrency` is a near-duplicate of `formatMoney` at src/utils/money.ts:12 (same structure, renamed identifiers/literals)",
  "confidence": 0.6
}
```

**Fix:** Call the existing implementation instead of duplicating it, or extract a shared helper both call. Trivial bodies (under ~60 normalized characters, e.g. one-line getters) are exempt; the Type-2 tier applies a higher floor still, since renaming shrinks what it compares. Two implementors of the same trait/interface sharing a body shape are exempt on both tiers. Disable with `enforce.duplication: false` in `keel.json`.

### W007 — Oversized File

**Severity:** WARNING

A compiled file exceeds the configured line budget (`enforce.max_file_lines`, default 400) and grew relative to the last `keel map`.

```json
{
  "code": "W007",
  "message": "File is ~512 lines (budget 400) and growing",
  "file": "src/services/order_service.ts",
  "line": 1,
  "hash": "i3Tx7kW9f0Q",
  "fix_hint": "Split src/services/order_service.ts into focused modules under 400 lines — run `keel analyze src/services/order_service.ts` for split suggestions, or delete what's no longer needed",
  "confidence": 0.8,
  "resolution_tier": "heuristic"
}
```

**Fix:** Split the file into focused modules, or delete unused code. Run `keel analyze <file>` for split suggestions. Shrinking a file that's already over budget doesn't re-trigger the warning. Disable with `enforce.oversized_files: false` in `keel.json`, or raise the budget with `enforce.max_file_lines`.

### W009 — New Cross-Boundary Dependency

**Severity:** WARNING

A compiled file calls into a package it did not depend on at the last `keel map`. An architecture decision is cheapest to reverse at the moment it is made, so this fires at edit time rather than showing up as an unexplainable dependency cycle months later.

```json
{
  "code": "W009",
  "message": "New dependency `harness` -> `core` via `raster_ingest`",
  "file": "crates/harness/src/run.rs",
  "line": 12,
  "hash": "i3Tx7kW9f0Q",
  "fix_hint": "`core` already exposes `execute` — go through it instead of reaching into `crates/core/src/ingest.rs`, or move the shared code into a boundary both already depend on",
  "confidence": 0.9,
  "resolution_tier": "heuristic"
}
```

How the check stays quiet:

- **Self-baselining.** Every boundary the graph already records the file's *module* (its directory) calling into is grandfathered, so only new erosion fires. There is no baseline file to maintain, and once a compile syncs the new edge into the graph the dependency stops being new.
- **One warning per boundary**, at the first reference that reaches it — not one per call site.
- **Declared boundaries only.** Nodes use their monorepo package; nodes without one fall back to the first path segment of their file (`frontend/src/x.ts` → `frontend`). A repo that declares no packages at all sees nothing, because a guessed boundary would produce confident wrong warnings.
- **Bootstrap guard.** Nothing fires before the first `keel map`, or in a directory whose stored nodes have no call edges yet. The guard is per module, not per file — a brand-new file in a mapped module does fire, since that is the likeliest way to introduce a violation.
- **Imported-by-name calls only.** The exact name has to appear in one of the file's imports. A fully-qualified path call (`other_crate::helper()`), a namespace import (`import * as core`), and Go's `pkg.Func()` are therefore invisible: this check would rather miss a dependency than invent one.
- **Public, non-associated targets, unambiguously resolved.** A method or associated function (`from`, `collect`, `get_edges`) is dispatch, not a named dependency, and a private function cannot be called across a boundary at all. A name matching functions in several packages is dropped rather than guessed.
- **`calls` only.** Type-only references are a dependency on an abstraction, which is the behaviour you want; count them with `architecture.count_type_deps: true`.

Together those filters keep the detector no wider than the graph it is diffed against: anything W009 could see but `keel map` cannot resolve would otherwise be reported on an unchanged tree forever.

**Fix:** Go through the target boundary's public surface (the fix_hint names its most-called public symbol), or move the shared code into a boundary both sides already depend on. To make a specific pair a hard error, list it under [`architecture.deny`](#e006--layer-violation).

## Plan findings

`P001` and `P002` live in a deliberately separate namespace. They are produced by [`keel validate-plan`](commands.md) only — never by `keel compile` — because they describe claims about code that does not exist yet. They never appear in the compile stream, never affect a compile's exit code, and `keel validate-plan` itself still exits 0 by default: the never-fails contract is intact unless you pass `--strict`.

### P001 — Unknown Symbol

**Severity:** WARNING (advisory)

The plan calls a symbol no graph node answers to — the hallucinated-callee case.

```json
{
  "code": "P001",
  "category": "unknown_symbol",
  "symbol": "computeTotals",
  "message": "Plan calls `computeTotals(rows)` but no symbol named `computeTotals` exists in the graph",
  "claimed": "computeTotals(rows)",
  "fix_hint": "Run `keel search computeTotals` to find the real name. If it is new code, say so in the plan (\"add `computeTotals`\") so keel treats it as proposed rather than missing.",
  "confidence": 0.6
}
```

How the check stays quiet (precision over recall — a plan is prose, and almost every English word is a valid identifier):

- **Bare call syntax only.** The name has to be a maximal identifier immediately followed by `(`. A dotted or path-qualified call (`rows.map(f)`, `serde_json::from_str(x)`) and a macro (`println!(...)`) are ignored: those are overwhelmingly stdlib or third-party, and attributing one to this repo is exactly the false signal keel exists to remove.
- **Short and non-ASCII names are excluded** — the single widest filter, and it applies to `P002` as well. A call claim shorter than three characters is never checked, whether or not the graph knows it: `t("nav.home")`, `cb(err, res)`, `f(x)`, `ok(value)` and even "moved out `of(pkg)`" all parse as calls, and nothing in the text distinguishes them from a real one. Identifiers containing non-ASCII characters are skipped for the same reason.
- **Proposed names are excluded.** Any call target named on a line carrying a creation verb (`add`, `create`, `new`, `implement`, `define`, ...), or preceded anywhere in the plan by definition syntax (`fn foo`, `def foo`, `function foo`, `class foo`, ...), is excluded from the whole plan. A function created in step 1 is legitimately called in step 3.
- **Keywords and builtins are excluded**, across all four languages plus the common JS/TS test DSL. The list is only consulted for names the graph cannot resolve, so it can never hide a real repo symbol.
- **All-caps names are excluded.** A name with no lowercase letter (`TODO(x)`, `MAX(a, b)`) is a placeholder, a constant or SQL, not a call into this repo.
- **A plan that resolved nothing is skipped entirely.** If no token in the plan matches any graph symbol, the plan is about another repo or the graph is stale; firing on every word would be noise.
- At most 20 findings per plan, one per symbol.

### P002 — Signature Mismatch

**Severity:** WARNING (advisory)

The plan's call does not match the stored `GraphNode.signature`. This is the "planning against a remembered API" failure — caught before the code exists rather than after 2,000 lines of it.

```json
{
  "code": "P002",
  "category": "signature_mismatch",
  "symbol": "execute",
  "message": "Plan signature for `execute` does not match the graph: claimed 2 argument(s), stored signature takes 1 (`execute(&self, sql: &str) -> Result<()>`)",
  "hash": "i3Tx7kW9f0Q",
  "file": "crates/db/src/exec.rs",
  "line": 42,
  "claimed": "execute(cmd, params)",
  "actual": "execute(&self, sql: &str) -> Result<()>",
  "fix_hint": "Use the stored signature `execute(&self, sql: &str) -> Result<()>` (crates/db/src/exec.rs:42); run `keel discover i3Tx7kW9f0Q` to see its 345 caller(s) before planning a change.",
  "confidence": 0.9
}
```

v1 compares **name + arity + return presence only**:

- **The receiver is normalized out on both sides** (`self`, `&self`, `&mut self`, `cls`, `this`), or every Rust method would mismatch every TypeScript function. Full normalization across Rust lifetimes/generics and TS type parameters is a follow-up behind per-language tests.
- **Return presence is compared only when the plan spells it.** An explicit `-> T` after the call counts; a bare `foo(x)` says nothing about the return and is not compared. A `:` after a call is not read as a TypeScript return annotation — in markdown it is far more often prose.
- **Uncountable parameter lists are skipped**: variadics (`*args`, `...rest`), defaults (`=`), optionals (`?`), and an elided `foo(...)`. Generic commas (`HashMap<String, u32>`) are not counted as argument separators.
- **A qualified call is only checked against an associated function or method.** `store.execute(a, b)` is checked; `rows.map(f)` is not matched against a free function named `map`.
- **Same-named candidates must agree.** If two nodes share the name and disagree on arity or return presence, the claim cannot be attributed and nothing fires.
- **A declared intent to change the signature is not a mismatch.** When the plan already says it is renaming, removing, or changing the signature of that symbol, the shape it is changing *to* is not reported.

### Strict mode and the plan hook

`keel validate-plan --strict` exits 1 when a live P001/P002 finding is present. It is opt-in everywhere:

- `keel init` scaffolds a Claude Code `PreToolUse` hook on `ExitPlanMode` (`.keel/hooks/plan-check.sh`) that pipes the plan into `keel validate-plan --llm -` and prints findings to stderr. It **always exits 0** — no session is ever blocked by the default config.
- `KEEL_PLAN_STRICT=1` turns the hook blocking (exit 2, stderr shown to the model).
- `KEEL_PLAN_HOOK=0` is the one-line bypass.

Repeat findings route through the same [circuit breaker](#circuit-breaker) as compile violations, keyed on the P-code plus the symbol and fingerprinted by the claim text. Three genuinely different but still-wrong claims downgrade the finding to INFO, which drops it out of `--strict`'s exit code — a stubborn claim degrades to advice instead of deadlocking a session. A correct plan clears the counter.

## Info

### S001 — Suppressed

**Severity:** INFO

A violation was suppressed via `--suppress` flag or circuit breaker auto-downgrade.

Only visible with `--verbose`. Indicates that a check was intentionally skipped for this run.

## Circuit Breaker

When the same error-code + hash pair fails repeatedly:

| Attempt | Behavior |
|---------|----------|
| 1 | Normal error with `fix_hint` |
| 2 | Error with wider `discover` context |
| 3+ (configurable) | Auto-downgraded to WARNING |

The counter resets on success or when a different error occurs on the same symbol. Configure the threshold in `keel.json`:

```json
{
  "circuit_breaker": {
    "max_failures": 3
  }
}
```

## Dynamic Dispatch

Low-confidence call edges from trait dispatch (Rust), interface methods (TypeScript/Go), or duck typing (Python) produce **warnings, not errors**. This prevents false positives on ambiguous resolution. Use `keel explain <code> <hash>` to inspect the resolution tier and confidence score.
