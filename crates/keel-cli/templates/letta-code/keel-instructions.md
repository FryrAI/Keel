<!-- keel:start -->
## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it. The hash is shown in the keel map (injected at session start).
- For **body-only changes** (bug fixes, refactoring internals), skip discover — compile will catch any issues.
- If the function has upstream callers (up > 0), you MUST understand them before changing its interface.

### After every edit:
- `keel compile` runs automatically via hooks after every Edit/Write/MultiEdit.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.
- If a warning has `confidence` < 0.7, attempt one fix. If it doesn't resolve, move on.

