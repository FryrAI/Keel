<!-- keel:start -->
## keel — Code Graph Enforcement

This project uses [keel](https://keel.engineer) for code graph enforcement.
keel validates structural integrity of the codebase via a code graph.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it.
- For **body-only changes** (bug fixes, refactoring internals), skip discover — compile will catch any issues.
- If the function has upstream callers (up > 0), you MUST understand them before changing its interface.

### After every edit:
- Run `keel compile <file>` to validate changes.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.

### Error codes:
| Code | Meaning |
|------|---------|
| E001 | broken_caller — a caller references a changed/removed function |
| E002 | missing_type_hints — function parameters or return type lack annotations |
| E003 | missing_docstring — public function lacks documentation |
| E004 | function_removed — a function was deleted but callers remain |
| E005 | arity_mismatch — caller passes wrong number of arguments |
| W001 | placement — function is in a non-ideal module |
| W002 | duplicate_name — another function with the same name exists |
| W005 | dead_code — private function has no callers in the graph |
| W006 | duplicate_implementation — function body is identical to one elsewhere |
| W007 | oversized_file — file exceeds the configured line budget and grew |
| S001 | suppressed — violation suppressed via `--suppress` or circuit breaker |

