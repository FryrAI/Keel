# Changelog

All notable changes to keel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `keel hook <name>` runs agent hooks at the project root from any directory, with a main-checkout fallback for linked worktrees.

### Changed
- Generated hook commands work from any directory inside the project.
- `keel init --update-docs` migrates existing generated JSON hook commands and installs a missing session-start script.

## [0.7.0] - 2026-10-10

Four new codes — opt-in expression homes (W011/E007), template-respelling
advisories (W012) and the SECURITY DEFINER `search_path` check (W013) — plus
graph-integrity fixes: one kernel-backed lock for every graph writer, one
project root whatever the cwd, and call resolution that never binds a bare
call to a member or across languages.

**Upgrading.** Run `keel map` once after installing: it populates the W012
template homes and rebuilds edges under the corrected resolution and package
rules. Upgrade every keel that writes the same `.keel` graph together —
0.6.2 and older writers do not reliably exclude 0.7.0 writers under the new
lock. Building from source needs Rust 1.89.

### Added
- **SECURITY DEFINER `search_path` rule (#104): W013.** `keel compile` warns on every SQL routine
  declared `SECURITY DEFINER` whose effective `search_path` is missing or does not end with `pg_temp`
  (PostgreSQL searches the temporary schema first otherwise). The effective list follows later
  `ALTER FUNCTION … SET/RESET search_path`, replacement and `DROP` statements across the directory's
  SQL files in migration order. Additive: no config, exit code unchanged without `--strict`; the
  compile JSON `category` enum gains `definer_search_path`.
- **Template respelling advisories (#87): W012.** `keel review` and MCP review
  report baseline-new literals containing at least 24 characters of a pure
  template function's fixed expression text, once per literal/home. Human, LLM,
  and JSON output name all owners; empty advisory lists stay silent. W012 never
  enters violations, gates, compile, or the circuit breaker. Independent labels on a
  private production corpus measured conservative precision 0.76, selecting
  advisory-only rollout; the method, counts, decision and keel's own exports
  live under `docs/studies/template-respelled/`. Run `keel map` once after
  upgrading to populate homes; review stays silent until that first map.
- **Expression homes follow-ups (#87, part A).** Patterns accept case-sensitive
  literals and line-local `{"regex": "…"}` objects; invalid, empty, or empty-matching
  regexes skip their rule, and explicit config writes refuse invalid objects.
  Syntax-tree comments and hash-bang lines are excluded in Rust, Python, Go,
  TypeScript/TSX, JavaScript/JSX, and Bash, while strings and Python docstrings
  remain checked. SQL, Typst, and raw Svelte/Astro markup remain unmasked.
- `keel compile` (and every git-diff target filter) now honours nested `.keelignore`/`.ignore`/`.gitignore`
  files exactly as `keel map` does: ignore-file kinds rank `.keelignore` > `.ignore` > `.gitignore`
  (deepest directory wins within a kind), a nested repository ends `.gitignore` inheritance, and an
  excluded directory stays excluded (#90).
  Existing source paths in a git-derived list (`compile --changed`/`--since`, `keel review`) take the
  walker's own verdict, so hidden, `.git/info/exclude`d and non-regular (symlink) paths are skipped exactly
  as `keel map` does; deletions and rename sources take the documented model; files keel cannot parse are
  filtered by explicit ignore rules only.
- **Opt-in expression homes (#80): W011/E007.** Literal patterns in code, strings,
  and test source are checked against home/scope globs. Tolerant rules
  preserve unrelated config. Compile compares with HEAD or `--since`; CLI/MCP
  review uses an immutable Git base and rename-aware multiset subtraction.
  The committed ratchet requires configured `review.gate` codes plus
  `keel review --base origin/main --gate`; a fresh map cannot re-baseline a PR.
  Server/watch/HTTP/MCP compile do not run homes.
  Removed occurrences cancel matching additions across the invocation's checked
  files: review considers all diffed files; compile considers its selected files.
  A compile of only a move's destination still reports it unless Git detects a rename. Symlink
  aliases are skipped; their targets are checked under their own paths.

### Fixed
- Renames to unreadable non-UTF-8 paths retain contract removals without
  cancelling newly introduced expression-home occurrences elsewhere.
- Git diff paths use NUL delimiters, so non-ASCII, tab, newline, and quote-containing
  filenames are checked correctly and indexed renames retain their old paths.
- JavaScript homes masking preserves JSX text in `.js`, `.mjs`, and `.cjs` files.
- Compile homes detect indexed renames against the immutable HEAD/`--since`
  base and consume each old blob once, preventing a separately selected deletion
  from cancelling a newly added occurrence. Plain unstaged moves remain undetected.
  Comment deletion preserves unchanged code identities; uncommenting matching
  code introduces a finding. Older breaker fingerprints reset on first sight of
  masked `homes-v3:` identities without charging an attempt or keeping a downgrade.
- Compile and file queries use one worktree-relative graph key, including through
  directory symlinks, from any working directory (#82/#86). A full `keel map`
  walks the entire project root even when invoked from a package directory, so
  remapping cannot erase files outside that package. `keel init` retains local
  tool detection, agent documents, ignore files and hooks in the invoking directory.
- Every disk-backed graph writer now shares a kernel-backed graph lock (#78),
  released automatically after a crashed holder on Unix and Windows. CLI writers,
  HTTP/MCP compile/fix/checkpoint, and watcher prune/compile cannot interleave with
  a map. Watchers retain busy batches without blocking the async runtime, and
  lock I/O errors are reported separately from contention.
- Review resolves `--base` to one immutable commit and refuses range expressions
  such as `main...HEAD` with an explicit error (exit 2).
- Review invoked from a subdirectory analyzes paths from the worktree root.
- Compile clears persisted circuit-breaker state when the last counter clears.
- Home violations charge one breaker attempt per file, using the identity set of
  the file's own surplus before move cancellation. Only a new identity advances
  the counter; unchanged sets keep it steady, and strict subsets reset it.
- Config writers patch only their selected raw JSON key, preserving malformed
  rules, string globs and unknown settings; invalid JSON is never overwritten.
  JSON formatting may change when rewritten. Unknown `enforce.homes` values warn
  and default to warning without discarding other settings. Home/scope globs
  drop leading `/`, empty and `.` components; `..` rejects the rule with a named
  warning. Root scope is allowed, root home warns and rejects the rule. Review
  gates naming the unavailable home code warn.
- `keel init --merge` and `keel push` refuse an unparsable existing `keel.json`
  with exit 2 instead of overwriting it. `keel config` validates `enforce.homes`
  and requires `homes` to be a JSON array before writing.
- Empty-hash circuit-breaker findings omit the unusable `keel discover` hint.

### Changed
- Minimum supported Rust version is now 1.89 for the standard file-locking API.

## [0.6.2] - 2026-09-04

Agent-hook reliability: every silent failure mode found while running keel
under parallel Claude Code and Codex agents, fixed at the root.

### Fixed
- **The post-edit hook never blocks with empty stderr (#73).** The installed
  `.keel/hooks/post-edit.sh` captured keel's output with a bare assignment
  under `set -e`, so every non-zero keel exit terminated the script before
  the diagnostic could be written — the "hook blocking error … No stderr
  output" reports. The hook now maps keel exit 1 (violations) to a blocking
  exit 2, keel exit 2/124 (internal error, timeout) to a non-blocking exit 1
  with the reason, always prints something on failure, and skips absolute
  paths outside the working tree before any other check. `keel compile`
  itself drops explicitly named targets outside the working tree with one
  stderr line (`compile_scope::screen_targets`, rooted at the new
  `keel_core::paths::worktree_root`).
- **`keel map` no longer fails with `UNIQUE constraint failed: nodes.id`
  (#69).** Not an upgrade-path bug: map cleared the graph and restarted ids
  at 1 without holding `.keel/compile.lock`, so a hook-triggered compile in
  between allocated the same ids. Map now holds the shared lock from before
  `clear_all` through its last write (waits up to 10 s, exits 2 naming the
  lock rather than skipping), prints a recovery recipe if the write still
  fails, and the PID-lockfile protocol is theft-proof (unparseable locks are
  held not stale, stale locks are claimed by atomic rename, release is
  ownership-checked). OS-level locking and the server/watch writers are
  tracked in #78.
- **Git-diff-driven commands honor `.keelignore` (#70).** `keel compile
  --changed/--since`, a bare compile in a git repo, `keel audit --changed`,
  `keel checkpoint` and `keel review` take their file list from git, which
  lists tracked-but-ignored files the walker never visits — so a vendored
  tree excluded by `.keelignore` raised violations in the pre-commit hook.
  One matcher (`keel_parsers::walker::KeelIgnore`) is applied inside
  `keel_enforce::gitdiff`, rooted at the repository top level, with the
  walker's semantics: `.keelignore` outranks `.gitignore`, a negation cannot
  re-include a file under an ignored directory, and a rename across the
  ignored boundary is reported as an addition or a deletion.
- **`keel init --update-docs` no longer creates an unwired
  `.keel/hooks/post-edit.sh` (#72).** It refreshes the hook only where one is
  already installed, checked at the path the installer writes, so the honest
  compile note in the refreshed docs can never flip to "runs automatically"
  in a project that wired no hook.
- **An empty directory named `.git` no longer counts as a repository root
  (#79).** `keel_dir`/`worktree_root` accept a `.git` entry only if it is a
  directory holding `HEAD` or a linked-worktree pointer file; a sandbox
  marker or stray `mkdir .git` in an ancestor used to hijack every project
  beneath it.

## [0.6.1] - 2026-08-26

### Added
- Parsers for Typst, Astro, Bash and SQL (`SupplementalResolver`).
- Semantic sprawl advisories and reuse candidates: W010 in `keel review`,
  P003 in `keel validate-plan`, `keel name --semantic` (#71).

## [0.6.0] - 2026-08-08

Slop-free keel: the SlopCodeBench-motivated release (arxiv.org/abs/2603.24755).
Agents' pass rates stay flat while structure erodes; this release gives keel
the metrics that see it and the detectors that catch it at generation time.

### Added
- **Per-function cyclomatic complexity (#58)**, computed at parse time for all
  four languages and stored on nodes. `keel quality` gains
  `high_cc_mass_share` — the share of `cc·√sloc` mass held by functions with
  cc>10, the paper's "structural erosion" score — and **`propagation_cost`
  (#61)**, the average transitive reachability of the module graph. Metrics
  added after v1 are named in `METRICS_ADDED_AFTER_V1`: a snapshot predating a
  metric is reported as unmeasured (`—`), never trended from a fabricated 0.
- **W006 Type-2 clone detection (#59).** A lexical fingerprint that renames
  identifiers positionally and collapses literals, so copy-paste-then-rename
  duplicates are caught (confidence 0.6, only when Type-1 finds nothing).
  Floor calibrated on keel's own tree — which promptly surfaced three real
  duplicate pairs in keel itself, deduplicated in the same PR.
- **Fragment-level clone detection (#66).** Rabin-Karp over 50-token windows
  at map time (pure Rust — no jscpd, no Node): sub-function block clones W006
  cannot see, reported as the trend-only `clone_loc_ratio` quality metric.
  The compile hot path is untouched.
- **`trivial_wrapper` audit smell (#60):** a single-delegating-call function
  with at most one caller, detected at parse time and persisted on the node.
- **`keel review --annotations-file` (#63):** one review run produces both
  the primary output and the GitHub annotations; the CI action no longer runs
  review twice.
- **`keel quality --export/--import` (#64):** JSON Lines round-trip of the
  quality-snapshot series (upsert by commit sha; a corrupt file imports
  nothing rather than half). The CI action accumulates history across runs
  via a cache, so `--trend` works in CI.
- **`perf-contract` CI job (#65):** the release-mode benchmark asserts that
  guard the documented <200ms single-file compile now actually run in CI.
  (Measured reality: 36–43ms; the reported ~1s was a debug binary under
  parallel test load.)

### Fixed
- **E001 no longer phantom-fires on same-file duplicate names** (a free
  `search_graph` beside a `search_graph` method): definition↔node pairing
  goes through one shared binder (hash evidence first, refuse when genuinely
  undecidable) used by E001, W005, and progressive adoption.
- **W002's Cargo exemption is structural (#62):** duplicate names across
  DISTINCT Cargo compilation units (build scripts, binaries, examples,
  benches — by target identity, Rust-only) are exempt; the old false positive
  on `build.rs` vs `src/main.rs` `main` is gone, and modules of one
  multi-file target still fire.
- **Raw strings no longer blind the hashes.** Rust `r#"…"#` and Go backtick
  strings shifted quote parity in the comment stripper: the rest of the body
  was silently dropped from node hashes (edits there didn't change the hash)
  and could produce false Type-2 collisions. One scanner per literal form,
  shared by both lexers. Affected bodies re-hash on the next `keel map`.
- **Non-ASCII identifiers** (`café`, `变量`) are single renameable tokens, so
  Type-2's rename invariance holds on unicode codebases.
- **Hook-driven compiles no longer serve stale metric inputs:** all three
  node writers share `Definition::apply_parse_facts`, so complexity and
  context flags move with the hash instead of waiting for the next map.

### Changed
- `violations_util.rs` (945 lines) split into focused submodules; MCP
  handlers share `engine_lookup`/`lock_engine`; each body is fingerprinted
  once per compile and tokenized once per map.

### Fixed (pre-0.6.0, shipped on main)
- **E005 arity_mismatch is reachable (#54).** No production site ever set
  `Reference::resolved_to`, so the check compared nothing and E005 could not
  fire outside unit tests. `keel compile` now resolves each compiled file's
  call references against the pre-edit graph (the same ladder the edge sync
  runs) before enforcement, at error-tier confidence only — a heuristic match
  (unknown receiver, cross-package name, boundary surface) never escalates
  into an arity ERROR. Call sites now carry a parser-counted argument count
  (`Reference::call_arity`); splat/spread arguments, macro token trees, and
  markup-derived "calls" record none and are skipped rather than guessed at.
  Signatures are compared only when precisely countable (variadic, defaulted,
  and optional parameter lists are skipped), a target compiled in the same
  batch is judged against its freshly-parsed signature instead of the stored
  one, and a type-qualified call (`Base.__init__(self, x)`, `Rc::clone(&x)`)
  tolerates the explicitly-written receiver while a value-receiver call
  (`registry.register(self)`) is compared exactly. Wired into `keel compile`,
  `keel fix`, and `keel checkpoint`; the MCP server's `keel/compile` still
  compiles without a resolution pass and keeps E005 silent for now (the
  ladder lives in keel-cli, which the server cannot depend on).

Older releases: [0.5.0 and earlier](CHANGELOG/README.md).
