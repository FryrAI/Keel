# Decision — W012 template_respelled (issue #87 item 2)

Verdicts: independent Claude opus judges (not the implementer), one judge per threshold sample, applying the rubric
in [README.md](README.md#method). The private corpus's labelled samples are held privately.

| Tree | Threshold | Sample | TRUE | FALSE | UNSURE | Conservative p = TRUE/(TRUE+FALSE+UNSURE) |
|---|---:|---:|---:|---:|---:|---:|
| private corpus | 16 | 50 of 215 | 35 | 12 | 3 | 0.70 |
| private corpus | 24 | 50 of 127 | 38 | 12 | 0 | 0.76 |
| keel | 16 | 1 of 1 | 0 | 1 | 0 | 0.00 |
| keel | 24 | 0 | — | — | — | N = 0 |

Rule (fixed before the study): pick the threshold with the higher p on the private corpus (24: 0.76 > 0.70). WARNING
needs p ≥ 0.8 — not met. ADVISORY needs p ≥ 0.5 and the issue's own example found — `berliner_tag_sql` is a home
with 24 respellings, 12/12 sampled ones judged TRUE. ⇒ **W012 ships as a `keel review` advisory at
MIN_SEGMENT_CHARS = 24; it never gates and never enters `keel compile`.**

False-positive sources the judges named (follow-up precision work, not tuned here): a segment that is a bare column
token of a projection-list home, a keyword-only prefix ending where the table name is interpolated
(`NOT EXISTS (SELECT 1 FROM`), one conjunct of a different predicate over the same columns, and whole-query
boilerplate; plus spurious co-owners from shared segments.
