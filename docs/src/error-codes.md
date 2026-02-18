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
  "resolution_tier": 1
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
