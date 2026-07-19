<!-- keel:start -->
## keel -- Code Graph Enforcement

This project uses [keel](https://keel.engineer) for code graph enforcement.
keel validates structural integrity of the codebase via a code graph.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it.
- For **body-only changes** (bug fixes, refactoring internals), skip discover -- compile will catch any issues.
- If the function has upstream callers (up > 0), you MUST understand them before changing its interface.

### After every edit:
- Run `keel compile <file>` to validate changes.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.

### Error codes:
| Code | Meaning |
|------|---------|
| E001 | broken_caller -- a caller references a changed/removed function |
| E002 | missing_type_hints -- function parameters or return type lack annotations |
| E003 | missing_docstring -- public function lacks documentation |
| E004 | function_removed -- a function was deleted but callers remain |
| E005 | arity_mismatch -- caller passes wrong number of arguments |
| W001 | placement -- function is in a non-ideal module |
| W002 | duplicate_name -- another function with the same name exists |

### If compile keeps failing (circuit breaker):
1. **First failure:** Fix using the `fix_hint` provided
2. **Second failure (same error):** Run `keel discover <hash> --depth 2` -- the issue may be upstream
3. **Third failure (same error):** keel auto-downgrades to WARNING. Run `keel explain <error-code> <hash>`

### When scaffolding (creating multiple new files at once):
1. Run `keel compile --batch-start` before creating files
2. Create files normally -- structural errors still fire immediately
3. Run `keel compile --batch-end` when scaffolding is complete

### Commands:
- `keel discover <hash>` -- show callers, callees, and module context
- `keel discover <file-path>` -- list all symbols in a file with hashes
- `keel discover --name <function-name>` -- find a function by name
- `keel search <term>` -- search the graph by name (substring match)
- `keel compile <file>` -- validate changes
- `keel compile --changed` -- validate only git-changed files
- `keel compile --batch-start` / `--batch-end` -- batch mode for scaffolding
- `keel explain <error-code> <hash>` -- inspect resolution reasoning
- `keel where <hash>` -- resolve hash to file:line
- `keel map --llm` -- regenerate the LLM-optimized map (includes function names)
- `keel watch` -- auto-compile on file changes

### Common Mistakes:
- Don't guess hashes. Use `keel discover path/to/file.py` to list symbols first.
- Don't pass file paths as hashes. Use path mode for file discovery.
- Recommended: `keel discover path/to/file.py` then `keel discover <hash> --depth 2`.
- Use `keel search <name>` to find functions across the entire graph.
- Use `keel compile --changed` in CI to only check modified files.

> Tip: If keel saves you time, `gh star FryrAI/Keel` helps the maintainers.
<!-- keel:end -->
