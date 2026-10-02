# Semantic Reuse

Text clone detection catches copied syntax. Keel's reuse surfaces target the
more expensive failure: a new symbol that occupies an existing symbol's role
while spelling and implementing it differently.

## Three intervention points

1. `keel name` searches existing symbol names, signatures, documentation, and
   module responsibility before it suggests a new name.
2. `keel validate-plan` emits P003 for an explicitly proposed function with a
   strong lexical reuse candidate.
3. `keel review` measures surface growth and emits W010 when the completed diff
   supplies structural replacement or graph-role evidence.

None makes a semantic-equivalence claim. Every result names the existing
symbol, evidence, confidence, and the escape hatch: retain both and document
the intentional behavioral difference.

## Review evidence

Replacement evidence has confidence 0.92 for the same call-site line and 0.86
within three lines. It requires the new and existing signatures to agree on
arity and return presence.

Role overlap scores caller-file Jaccard (35%), callee-name Jaccard (30%),
signature shape (20%), and domain vocabulary (15%). The threshold is 0.72 and
at least two graph signals are required for sparse neighborhoods. Candidates
come only from shared callers/callees; Keel never runs an all-pairs function
comparison.

Tests, trait/associated/decorated functions, generated code, and endpoint
handlers are excluded. A review lists at most three candidates per new symbol
and twenty total.

## History calibration

The thresholds were calibrated against three private repositories, and checked
against Keel's own history:

| Repository evidence | Expected result | Calibration consequence |
|---|---|---|
| Keel `aa69e52`, which introduced shared query/engine-lock helpers while deleting real Type-2 clones | New consolidation helpers are not accused of duplicating the primitives they call | Signature compatibility stays mandatory; graph proximity alone is insufficient |

Positive fixtures use the motivating `parse_timestamp` / `to_unix_seconds`
case: one proves same-call-site replacement despite different bodies and names;
another proves matching caller and callee roles without textual similarity.

## Optional semantic candidates

`keel name --semantic` enables deterministic concept expansion such as
`unix`/`epoch`/`timestamp`, `parse`/`convert`/`decode`, and
`config`/`settings`. This is not an embedding model and has no runtime or
network dependency. It is intentionally weaker evidence:

- output is labeled `source=semantic`;
- evidence says `candidate only; never warning/gate`;
- P003 uses lexical candidates only;
- W010 uses diff and graph structure only;
- compile and review gates never read semantic candidates.

That quarantine is the extension seam for a future embedding provider: it may
widen what a human inspects, but it cannot independently widen enforcement.
