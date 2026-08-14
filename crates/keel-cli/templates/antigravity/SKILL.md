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
1. Run `keel name "<intent>" --llm` and inspect every `REUSE?` candidate (`--semantic` is optional candidate-only expansion)
2. Reuse a compatible symbol; only create code when the behavior is materially distinct
3. Place new code in the suggested module
4. Add type hints on all parameters and return type
5. Add docstring if the function is public
6. Run `keel compile <file> --json` to validate placement

## Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel compile <file> --json` — validate changes
- `keel explain <error-code> <hash>` — inspect resolution reasoning
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — token-optimized codebase map
- `keel name "<intent>" [--semantic] --llm` — reuse candidates before create-new suggestions; semantic candidates never warn or gate
