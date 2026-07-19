<!-- keel:start -->
## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.
No automatic hooks are available in Aider — you must run keel commands manually.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it.
- For **body-only changes**, skip discover — compile will catch any issues.
- If the function has upstream callers (up > 0), you MUST understand them before changing its interface.

### After every edit:
- Run `keel compile <file> --json` to validate changes.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.

### If compile keeps failing (circuit breaker):
1. **First failure:** Fix using the `fix_hint` provided
2. **Second failure (same error):** Run `keel discover <hash> --depth 2` — the issue may be upstream
3. **Third failure (same error):** keel auto-downgrades to WARNING. Run `keel explain <error-code> <hash>`

### Before creating a new function:
1. Check the keel map (`keel map --llm`) to see if a similar function already exists
2. Place the function in the module where it logically belongs

### Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel compile <file>` — validate changes (MUST run after every edit)
- `keel compile --batch-start` / `--batch-end` — batch mode for scaffolding
- `keel explain <error-code> <hash>` — inspect resolution reasoning
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — regenerate the LLM-optimized map
<!-- keel:end -->
