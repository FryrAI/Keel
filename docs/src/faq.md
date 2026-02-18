# Frequently Asked Questions

## Performance

### "keel compile is slow"

It should not be. `keel compile` targets <200ms for a single file. Check:

1. **Are you running a release build?** Debug builds are 10-20x slower. Build with `cargo build --release` or install with `cargo install --path crates/keel-cli`.
2. **Is the graph database corrupted?** Run `keel map` to rebuild the graph from scratch.
3. **Are you compiling too many files?** `keel compile` without arguments scans all source files. Pass specific file paths for incremental validation: `keel compile src/auth.ts`.

### "keel map takes a long time"

`keel map` targets <5s for 100k LOC. If it is slower:

1. Check `.keelignore` -- make sure `node_modules/`, `vendor/`, and other large directories are excluded.
2. Large monorepos may hit I/O limits. The graph is built in parallel using rayon, but disk speed still matters.

## Resolution

### "keel can't find my function"

1. Run `keel map` to refresh the graph. The function may have been added after the last map.
2. Check `.keelignore` -- the file containing the function may be excluded.
3. Verify the file has a supported extension (`.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.go`, `.rs`).
4. Use `keel where <hash>` to check if the hash resolves. If not, the function may not have been parsed.

### "False positive violation"

keel's circuit breaker handles repeated false positives automatically:

- **Attempt 1:** Reports the error with a `fix_hint`.
- **Attempt 2:** Reports with wider `discover` context.
- **Attempt 3 (default `max_failures`):** Auto-downgrades the violation to a WARNING.

The counter resets when the error is resolved or a different error occurs on the same symbol.

To investigate: run `keel explain <error_code> <hash>` to see the full resolution chain and understand why the violation fired.

To suppress a specific check for one run: `keel compile --suppress E002 src/legacy.py`.

### "Low-confidence warning on trait/interface dispatch"

This is expected. Dynamic dispatch (trait methods in Rust, interface methods in TypeScript/Go) produces low-confidence call edges. keel reports these as **warnings, not errors** to avoid false positives. Use `keel explain` to inspect the resolution tier.

## Configuration

### "How do I ignore a file?"

Add the path or pattern to `.keelignore` at the project root. The syntax is identical to `.gitignore`:

```
# Ignore a specific file
src/generated/schema.ts

# Ignore a directory
test/fixtures/

# Ignore by pattern
*.generated.ts
```

After modifying `.keelignore`, run `keel map` to rebuild the graph without the excluded files.

### "keel init says .keel/ already exists"

Run `keel deinit` first to remove the existing configuration, then run `keel init` again:

```bash
keel deinit
keel init
```

### "How do I change enforcement rules?"

Edit `.keel/keel.json`. For example, to disable docstring enforcement:

```json
{
  "enforce": {
    "type_hints": true,
    "docstrings": false,
    "placement": true
  }
}
```

Changes take effect on the next `keel compile` or `keel map`. No restart needed.

## General

### "Does keel work offline?"

Yes, 100%. keel is a single binary with zero runtime dependencies and no network calls. Everything runs locally against your filesystem and the `.keel/graph.db` SQLite database.

### "Does keel modify my code?"

Only with `keel fix --apply`. All other commands are **read-only**. They parse and analyze your code but never write to source files.

`keel fix` without `--apply` outputs a plan but does not touch any files. Only `keel fix --apply` writes changes to disk, and it re-compiles afterward to verify the fix is clean.

### "What's the hash format?"

Every symbol gets an 11-character hash: `base62(xxhash64(canonical_signature + body_normalized + docstring))`. The hash is deterministic -- the same code always produces the same hash. It changes when the function signature, body, or docstring changes.

Example: `a7Bx3kM9f2Q`

### "How do I update keel?"

```bash
# From source
cargo install --path crates/keel-cli --force

# Via install script
curl -fsSL https://keel.engineer/install.sh | bash

# Via Homebrew
brew upgrade keel
```

After updating, run `keel map` to rebuild the graph with the new version.

### "Can I use keel without AI tools?"

Yes. keel works as a standalone structural linter. Common non-AI workflows:

- **Pre-commit hook:** `keel init` installs one automatically. Every commit runs `keel compile`.
- **CI pipeline:** Add `keel compile --strict --json` to your CI. Exit 1 fails the build.
- **Manual validation:** Run `keel compile` from the terminal whenever you want.

### "Which languages are supported?"

TypeScript, JavaScript (with JSDoc), Python, Go, and Rust. keel uses tree-sitter for universal parsing and per-language enhancers for deeper resolution:

| Language | Tier 1 (tree-sitter) | Tier 2 (Enhancer) |
|----------|---------------------|-------------------|
| TypeScript/JavaScript | tree-sitter-typescript | Oxc (oxc_resolver + oxc_semantic) |
| Python | tree-sitter-python | ty (subprocess) |
| Go | tree-sitter-go | tree-sitter heuristics |
| Rust | tree-sitter-rust | rust-analyzer (lazy-load) |

### "What does 'clean compile' mean?"

Zero errors + zero warnings = exit code 0, **empty stdout**. This means the structural graph is consistent and all contracts are satisfied. The LLM agent (or human) sees nothing and can move on.
