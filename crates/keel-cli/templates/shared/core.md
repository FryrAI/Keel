### Error codes:
| Code | Meaning |
|------|---------|
| E001 | broken_caller — a caller references a changed/removed function |
| E002 | missing_type_hints — function parameters or return type lack annotations |
| E003 | missing_docstring — public function lacks documentation |
| E004 | function_removed — a function was deleted but callers remain |
| E005 | arity_mismatch — caller passes wrong number of arguments |
| E006 | layer_violation — dependency denied by `architecture.deny` in keel.json (opt-in) |
| W001 | placement — function is in a non-ideal module |
| W002 | duplicate_name — another function with the same name exists |
| W005 | dead_code — private function has no callers in the graph |
| W006 | duplicate_implementation — function body is identical to one elsewhere |
| W007 | oversized_file — file exceeds the configured line budget and grew |
| W009 | new_cross_boundary_dep — this file now depends on a package it did not before |
| S001 | suppressed — violation suppressed via `--suppress` or circuit breaker |

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
- `keel map --semantic` — per-file summaries, public API, and when-to-use guidance
- `keel watch` — auto-compile on file changes
- `keel check <hash>` — pre-edit risk assessment (callers, risk level)
- `keel fix [--apply]` — generate and optionally apply fix plans
- `keel name <description>` — suggest names for new code
- `keel analyze <file>` — architectural analysis of a file
- `keel audit [--top N]` — AI-readiness scorecard (structure, discoverability, navigation, config); ranked worst-first, top 20 by default, `--top 0` for all
- `keel context <file>` — minimal structural context for safely editing a file
- `keel skeleton <file>` — compressed signature-only view (`--docs`, `--private`, `--budget <tokens>`)
- `keel focus <hash|file>` — minimal context set to safely modify a target (`--depth N`, `--budget <tokens>`)
- `keel checkpoint [--since <commit>] [--staged] [-o <file>]` — compact session-state summary (changed symbols, affected callers, violations, recent commits) for re-injection after context loss
- `keel validate-plan <file|->` — validate a plan against the graph before execution (callers at risk, risk level, callers-first order)
- `keel review --base <ref>` — two-sided graph diff vs a base ref: which contracts moved, which callers were left outside the diff, and which violations the diff *introduced* (`--format github` for CI annotations, `--gate` to fail on the codes listed in `review.gate`)

**Tip:** When running keel commands manually, always use the `--llm` flag for token-efficient output.

### MCP Tools (available via `keel serve --mcp`):
The keel MCP server exposes these tools directly to your IDE:
- `keel/compile` — compile files and check for violations
- `keel/discover` — find callers and callees of a function by hash
- `keel/where` — resolve a hash to file:line
- `keel/explain` — explain a violation with resolution chain
- `keel/map` — full or scoped graph map
- `keel/check` — pre-edit risk assessment
- `keel/fix` — generate fix plans for violations
- `keel/search` — search the graph by name
- `keel/name` — suggest names for new code
- `keel/analyze` — architectural analysis of a file
- `keel/audit` — AI-readiness scorecard
- `keel/context` — minimal structural context for a file
- `keel/skeleton` — compressed signature-only view of a file
- `keel/focus` — minimal context set to safely modify a target
- `keel/checkpoint` — compact session-state summary for re-injection after context loss
- `keel/validate-plan` — validate a plan against the graph before execution
- `keel/review` — two-sided graph diff vs a base ref (contracts moved, callers left behind, violations the diff introduced)

### Common Mistakes:
- **Don't guess hashes.** Use `keel discover path/to/file.py` to see all symbols and their hashes first.
- **Don't pass file paths as hashes.** If discover says "hash not found", check if you passed a file path — use path mode instead.
- **Recommended workflow:** `keel discover path/to/file.py` → see all symbols → `keel discover <hash> --depth 2` for deep exploration.
- **Use `keel search`** to find functions by name across the entire graph.
- **Use `--changed` in CI** to only check modified files: `keel compile --changed`. Add `--format github` there to get inline PR annotations straight from the binary.
