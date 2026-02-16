<!-- keel:start -->
## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it. The hash is shown in the keel map (injected at session start or embedded below).
- For **body-only changes** (bug fixes, refactoring internals, improving logging), skip discover — compile will catch any issues.
- If the function has upstream callers (↑ > 0), you MUST understand them before changing its interface.

### After every edit:
- `keel compile` runs automatically via hooks after every Edit/Write/MultiEdit.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.
- If a warning has `confidence` < 0.7, attempt one fix. If it doesn't resolve, move on.

### If compile keeps failing (circuit breaker):
1. **First failure:** Fix using the `fix_hint` provided
2. **Second failure (same error):** Run `keel discover <hash> --depth 2` — the issue may be upstream
3. **Third failure (same error):** keel auto-downgrades to WARNING. Run `keel explain <error-code> <hash>` to inspect the resolution chain.

### Before creating a new function:
1. Check the keel map to see if a similar function already exists
2. Place the function in the module where it logically belongs
3. If keel warns about placement, move the function to the suggested module

### When scaffolding (creating multiple new files at once):
1. Run `keel compile --batch-start` before creating files
2. Create files normally — structural errors still fire immediately
3. Run `keel compile --batch-end` when scaffolding is complete

### Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel discover <file-path>` — list all symbols in a file with hashes
- `keel discover --name <function-name>` — find a function by name
- `keel search <term>` — search the graph by name (substring match)
- `keel compile <file>` — validate changes
- `keel compile --changed` — validate only git-changed files
- `keel compile --since <commit>` — validate files changed since a commit
- `keel compile --batch-start` / `--batch-end` — batch mode for scaffolding
- `keel explain <error-code> <hash>` — inspect resolution reasoning
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — regenerate the LLM-optimized map (includes function names)
- `keel watch` — auto-compile on file changes

### Common Mistakes:
- **Don't guess hashes.** Use `keel discover path/to/file.py` to see all symbols and their hashes first.
- **Don't pass file paths as hashes.** If discover says "hash not found", check if you passed a file path — use path mode instead.
- **Recommended workflow:** `keel discover path/to/file.py` → see all symbols → `keel discover <hash> --depth 2` for deep exploration.
- **Use `keel search`** to find functions by name across the entire graph.
- **Use `--changed` in CI** to only check modified files: `keel compile --changed`.
<!-- keel:end -->
