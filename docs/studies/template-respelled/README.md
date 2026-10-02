# Template respelling precision study — W012 advisory decision

**Decision: W012 ships as a review-only advisory at 24 characters.** Independent
judges measured conservative precision 38/50 = 0.76 at 24 on a private production
codebase, versus 35/50 = 0.70 at 16. The selected threshold has no
occurrences in keel's own tree. It meets the advisory rule (precision at least 0.5
and the issue's own example found) but misses the warning rule (precision at least
0.8). W012 never gates, enters compile, or participates in the circuit breaker.
The ruling is in [DECISION.md](DECISION.md).

The private corpus's exports, samples, verdicts and reproduction script are not
published: they quote that codebase's source. Only its aggregate counts appear
here. keel's own populations are published in full below. Run
[`reproduce.sh`](reproduce.sh) to rebuild them with public phase-1 exporter
`a9e3ade` and the pinned keel source. The sample header preserves the original
unlabelled-export wording; the verdict cell records the independent review.

## Method

- Populations: the private corpus at a pinned commit, and keel at
  `911cca1748edaa8bb0056c50283ad1e41731060b`, each exported with `git archive`
  into a scratch repository with a parentless empty base commit.
- Exporter: `keel review --json --base empty-base` after a full `keel map`, using
  a study build whose payload sat outside `new_violations`, gates, compile and
  human/LLM rendering.
- Both thresholds come from the same complete 16-character export. Filtering to 24
  is exactly equivalent to extracting at 24: home eligibility does not depend on
  segment length, and matching, exclusion and subtraction are per segment. Lengths
  count Unicode characters, not bytes.
- Samples: Python's `random.Random(87).sample` over the stable occurrence order,
  independently per population, size `min(50, N)`. Export indices in the keel
  sample tables are zero-based indices into that threshold's `occurrences` array.
- Verdicts: independent judges (not the implementer) read every home and literal
  in context. **TRUE** only when the literal contains the home's complete
  expression with the home's meaning, so calling the home would be
  behaviour-preserving; a shared prefix, boilerplate or a different predicate is
  **FALSE**.

## Counts

| Tree | Minimum chars | Template functions | Distinct kept segments | Owned segments | Occurrences | Sample |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| private corpus | 16 | 117 | 88 | 90 | 215 | 50 |
| private corpus | 24 | 117 | 65 | 67 | 127 | 50 |
| keel | 16 | 11 | 4 | 4 | 1 | 1 |
| keel | 24 | 11 | 2 | 2 | 0 | 0 |

`template_functions` counts all eligible pure template functions, including those
whose segments do not survive the threshold. `kept_segments` counts distinct
segment texts; shared ownership is retained. These study counts use one
literal/segment pair with all owners attached. Production advisories are
deduplicated per literal/home **after** baseline subtraction. Multiple copies of
the same text at different literal nodes are separate occurrences.

The issue's example helper, `local_day_sql`, was detected as a home with 24
respelling occurrences at each threshold; all 12 sampled at 24 were judged TRUE.

## Keel exports

- [keel-16.json](keel-16.json): counts, all mapped homes, and all occurrences at 16.
- [keel-16-sample.md](keel-16-sample.md): seed-87 sample (its one row was judged FALSE).
- [keel-24.json](keel-24.json): counts, all mapped homes, and all occurrences at 24.
- [keel-24-sample.md](keel-24-sample.md): seed-87 sample (empty).

## Scope, decoding, and bounded staleness

Supported homes and occurrences: Rust, Python, TypeScript/TSX/JavaScript/JSX,
and Go whole-file grammars. Svelte, Astro, SQL, Typst, BAML, and Bash are excluded.
Test files and inline test contexts are excluded using the parser's shared context
logic. Non-test generated paths participate when the normal map walker includes
them. Comments cannot match. Module-level literals participate; Rust `concat!`
strings and Python adjacent literals are folded. `+` chains remain unsupported.

Only a single returned template expression qualifies as a home. Other statements,
branches, multiple exits, and side effects outside interpolations disqualify it;
nested callables are evaluated independently. Python docstring statements also
disqualify an otherwise pure body.

The `template_homes` table is created idempotently on open, with no schema version
bump, following `fragment_clones`' additive derived-cache precedent. It stores one
JSON home per row, including empty segment lists and multiple homes on one line.
Replacement is transactional. `clear_all()` clears it; quality snapshots survive.
Review reads the invocation-wide home population from the last full map, and
recalculates current home-body spans before excluding owners' literals. Cache
freshness has the same bounded staleness as clone measurements; remap after
changing homes. Compile has no W012 enforcement and there is no incremental cache
writer. Production ignores segments shorter than 24 even when reading an older
16-character map cache.

Standard escapes and recognized formatting expressions are decoded on both
sides. Physical CRLF is normalized to LF in Rust, Python and JS strings/templates;
Go raw strings discard CR, and Go hex/octal escapes assemble UTF-8 bytes. Byte
strings otherwise retain their written escape text. AST interpolation boundaries
prevent nested expressions or format specifications from leaking into fixed segments. `literal` displays holes using
NUL separators; `literal_parts` preserves the actual decoded pieces, so literal
NULs cannot match across holes or collide with interpolations during subtraction.
Baseline identity is the segment plus whitespace-normalized decoded pieces,
without hashes, filenames, or lines; removed copies cancel moves across the
changed-file set. Multiline findings use the literal's start line.

## Residuals

- Python named Unicode escapes (`\N{...}`) are rejected rather than guessed: a
  documented false negative. Numeric Unicode, hex, octal and the standard simple
  escapes are supported.
- Go `fmt.Sprintf` ownership uses a conservative file-level binding check: the
  standard `fmt` import is required, and a binding named `fmt` anywhere in the
  file refuses Sprintf homes, including a shadow in an unrelated scope.
- Known false-positive sources, not tuned away in this rollout: bare
  projection-column tokens, keyword-only prefixes ending where a table name is
  interpolated, one conjunct of a different predicate over the same columns,
  whole-query boilerplate, and spurious co-owners of shared segments.

W012 never fails a review: unreadable or non-UTF-8 files are skipped on both Git
sides, preserving advisories from other files. `--verbose` names each skipped
file and the read error. Owners deleted in the diff are dropped; owner-body
exclusion is recalculated independently on each side, including cached owners
whose current bodies no longer qualify as templates.
