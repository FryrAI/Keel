---
name: keel-enforcement
description: Use this skill when editing code, creating new functions, or refactoring. Validates structural integrity via keel code graph enforcement.
---

# keel Code Graph Enforcement Skill

## When editing existing code:
1. Find the function hash from the keel map (run `keel map --llm` if not in context)
2. Run `keel discover <hash>` to see callers and callees
3. Make the edit
4. Run `keel compile <changed-file> --json`
5. Fix any errors before continuing

## When creating new functions:
1. Run `keel map --llm` and check if a similar function exists
2. Place in the module with the best semantic fit
3. Add type hints on all parameters and return type
4. Add docstring if the function is public
5. Run `keel compile <file> --json` to validate placement

## Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel compile <file> --json` — validate changes
- `keel explain <error-code> <hash>` — inspect resolution reasoning
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — token-optimized codebase map
