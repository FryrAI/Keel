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

`keel map` records the commit it mapped in `keel_meta` (`last_map_commit`). If that
commit is not an ancestor of `HEAD`, `keel compile` **exits 2** with:

```
keel compile: the stored graph was built at commit 4f1c0a9b2d3e, which is not an
ancestor of HEAD — it describes code this checkout does not contain, so its callers
and removals would be phantom. Run `keel map` to rebuild the graph.
```

This is what makes a poisoned cache fail loudly instead of annotating fiction. It also
fires locally after a rebase, an amend, or a branch switch away from the mapped
history — the fix is the same: `keel map`.

The guard stays silent whenever it cannot be certain: a graph with no marker (mapped
by an older keel, or outside a git repo), no git, or a commit this clone does not
have. Existing graphs never start failing because of it.

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
