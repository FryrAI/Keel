## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.

### Session continuity (before compaction)
Context windows compact and reset. Before you run low on context, capture a compact,
re-injectable summary of the session's structural changes:

```
keel checkpoint --llm --since <session-base-commit> -o .keel/checkpoint.md
```

It records changed/added/removed symbols, callers now at risk, outstanding violations,
and recent commit subjects — all derived from git + the graph, no stored state. After a
reset, re-read `.keel/checkpoint.md` to recover where you were. (Keeping the file in a
project vault/notes directory is optional and entirely user-side.)

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

