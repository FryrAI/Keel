# CI (GitHub Actions)

One recipe. `keel init` scaffolds `.github/workflows/keel.yml`, and everything it
does lives in the maintained composite action — so when the recipe improves, you
get it without editing your workflow.

```yaml
name: keel

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read
  pull-requests: write # the sticky review comment

jobs:
  keel:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: FryrAI/Keel/.github/actions/keel@v0
        with:
          mode: auto
```

`.keel/keel.json` must be committed (it is not gitignored — only `graph.db` and the
other generated files are). Run `keel init` locally once and commit what it writes.

## What each event does

| Event | keel run | Why |
|---|---|---|
| `pull_request` | `keel review --base <merge base>` | Which contracts moved, which callers were left behind, and the violations **this diff** introduced. Annotations on the diff, plus one sticky comment. |
| `push` | `keel compile --changed` | On a clean checkout this finds nothing to compile — the payoff is that the job leaves the graph for that commit in the cache, so pull requests based on it start warm. |

`mode: auto` picks per event. `mode: review` / `mode: compile` force one; `args:`
overrides both (`args: compile --since HEAD~5`).

## `fetch-depth: 0` is required

`keel review` reads the base side of the diff straight out of git and computes the
merge base to key the graph cache on. A shallow clone has neither.

## Graph provisioning that cannot lie

The `keel-graph` action (used by the `keel` action, and reusable on its own) restores
`.keel/graph.db` from `actions/cache` keyed on the **merge-base commit SHA**, with no
prefix `restore-keys` fallback. That omission is the design:

- A prefix restore hands CI a graph built from *different source*. Enforcement against
  a stale graph manufactures phantom findings — renamed functions read as removed,
  live callers read as broken.
- A cache miss costs one `keel map` (announced in the log — roughly 14s on a 157k-LOC
  repo). A wrong graph costs trust in every annotation keel ever posts.

Only `push` runs write to the cache: a pull request maps at *its own* head, so saving
that under the merge-base key would poison every later PR sharing that base.

## The staleness guard

`keel map` records the commit it mapped in `keel_meta` (`last_map_commit`). When `HEAD`
has moved off that commit, a working-tree `keel compile` (explicit files, `--changed`,
or bare) **brings the graph to `HEAD` first**, from `HEAD`'s committed files, moves the
marker to `HEAD`, and then checks. Nothing is printed on success (with `--verbose`, one
line: `re-synced N file(s) from <old> to <new>`). It is what keeps a post-edit hook
working after a branch switch, a squash merge or a pull.

Definitions bind one-to-one by matching hashes, then by a unique remaining
name and module/non-module kind; ambiguous old nodes are replaced. Unchanged
callers of removed nodes re-resolve references without changing stored nodes: from `HEAD`'s blob and,
unless the file is a target of this compile, from its working-tree text (where both texts yield the
same call, the working-tree line is kept). A reference-only pass removes
no edge it does not re-add. A run that does not check its files — a batch control command, a
missing argument — has no targets. When a re-sync adds a definition, every modified or untracked file the graph
holds is re-resolved the same way.
Re-synced code is pre-existing, while committed refresh preserves session history.

It refuses instead, exit 2 with one line starting `keel compile: no usable graph — `,
when the move cannot be bridged: the stored commit is gone from this clone (run
`keel map`), more than 500 source files differ from `HEAD` (run `keel map`), more than
15 differ and the compile named explicit files (run `keel compile` with no arguments),
or the re-sync itself failed (run `keel map`). These limits include dependent caller
files and recorded recovery paths. A refusal leaves the marker unmoved and nothing
is checked; partial graph writes may remain. The next working-tree compile, at
whatever `HEAD`, completes or reverts them using the recorded paths. Completion
clears the record and moves the marker atomically.

`keel compile --since <ref>` keeps the old guard, because it checks a committed range
against the stored base graph and a re-sync would erase the differences it reports. If
the stored commit is not an ancestor of `HEAD`, it **exits 2** with:

```
keel compile: the stored graph was built at commit 4f1c0a9b2d3e, which is not an
ancestor of HEAD — it describes code this checkout does not contain, so its callers
and removals would be phantom. Run `keel map` to rebuild the graph.
```

This is what makes a poisoned cache fail loudly instead of annotating fiction.

While a re-sync is recorded as unfinished, `--since` also **exits 2**, with stdout empty and
one line starting `keel compile: no usable graph — `: the stored graph has an unfinished
re-sync, so run `keel compile` to finish it. Nothing is checked until then.

Every compile mode requires a completed map: explicit files, `--changed`, bare,
`--since`, `--delta`, and batch start/end. A repository is **never mapped** when
`last_map_at` is absent, or when `HEAD` exists and `last_map_commit` is absent. That
includes fresh clones, fresh `keel init`, interrupted maps, and a map made before
the first commit. Compile exits 2, writes no nodes, edges, markers or snapshot,
leaves stdout empty, and prints one stderr line. Without `last_map_at`:

```
keel compile: no usable graph — this repository has never been mapped (or the last `keel map` did not finish). Run `keel map` to build the graph; nothing was checked.
```

With `last_map_at` but no `last_map_commit`, when `HEAD` exists:

```
keel compile: no usable graph — the stored graph does not record the commit it was built at (it was mapped before the first commit, or by an older keel). Run `keel map` to rebuild it; nothing was checked.
```

`keel map` removes both markers before clearing the graph, and stamps them only
after the last graph write. Outside git, or before the first commit, a completed
map needs only `last_map_at`; compile can then run without a commit re-sync.
A stored commit this clone does not have makes working-tree compiles refuse until
`keel map` rebuilds the graph. The `--since` guard retains its legacy silence when
it cannot resolve an otherwise recorded commit.

## Cached maps and post-edit hooks

`keel map --cached` never runs a full map or takes the graph lock. On a never-mapped
repository it exits 2 with empty stdout and one stderr line starting
`keel map --cached: no usable graph — ` followed by the same reason and fix above. If its
stored commit differs from `HEAD` (including a commit missing from this clone),
it still prints the stored map and exits 0, with one stderr line naming both
commits by their first 12 characters and recommending `keel map`. A map at `HEAD`
keeps the same output.

The generated post-edit hook exits **2** when any line of compile output starts
with `keel compile: no usable graph — `, so the refusal reaches the agent even if
a version-drift warning precedes it. A prefix mentioned mid-line does not block.
Violations also block; other keel exit-2 errors and timeouts remain non-blocking
(hook exit 1), with their diagnostic on stderr.

## The delta baseline

Only `keel compile --delta` writes `.keel/last_compile.json`. Ordinary compiles
leave that baseline intact, so a whole-tree or background check cannot silence
the next post-edit hook's findings. A full `keel map` deletes the snapshot; the
next delta compile reports its full results and establishes a new baseline.
Repeating the same delta compile then reports only changes from that baseline.

## Linked git worktrees

A linked worktree (`git worktree add`) keeps its own graph, lock and delta baseline in
its git directory (`.git/worktrees/<name>/keel`), not in `.keel/`. Worktrees sit on
different branches, so they never share a graph. `keel.json`, `telemetry.db` and the
hook-script fallback still come from the main checkout's `.keel/`. A new worktree has no
graph: `keel compile` refuses there until `keel map` has run once in it. `keel deinit`
in a worktree removes only that worktree's graph. The graph is deleted with the worktree
by `git worktree remove`.

## One sticky comment

In review mode the action keeps a single comment on the PR, found by a hidden HTML
marker and **rewritten in place** only when a hash of the rendered body changes — a
re-push that changes nothing keel can see produces no new notification. It goes
through `gh api repos/{owner}/{repo}/issues/{n}/comments` (REST); `gh pr view`/`gh pr
edit` are GraphQL and 400 on repositories affected by the Projects-classic
deprecation.

Set `comment: false` to turn it off; the report still reaches
`$GITHUB_STEP_SUMMARY`.

## Forked pull requests

A PR from a fork runs with a read-only token, so it cannot comment. The action writes
the report to the step summary and the job stays green. keel does **not** use
`pull_request_target` for this — that hands fork code a write token, which is a
supply-chain hole, not a fix.

## Version drift

`map`, `compile`, and `review` each print one stderr line when the running binary
disagrees with `.keel/keel.json` (or with the version stamped into your generated
agent docs), pointing at `keel init --update-docs`. keel never rewrites those files
on its own.
