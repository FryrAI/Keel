//! Named confidence constants for call-edge resolution.
//!
//! A call edge's confidence is not decorative: it decides whether a broken
//! caller fires as an ERROR or a WARNING (see the error/warning tier split in
//! `keel-enforce`). Scattering the deciding values as bare literals across the
//! map and compile-sync resolution ladders makes that policy invisible and
//! lets the two pipelines drift. These constants are the single source of
//! truth for the tiers the resolution ladder assigns; the static assert below
//! pins the warning-tier values below the error threshold so a rename or a
//! typo cannot silently promote a heuristic edge to an ERROR.

/// A same-file direct call resolved against the file's own definitions — the
/// most certain non-exact resolution the ladder produces.
pub const SAME_FILE_CALL: f64 = 0.95;

/// A same-file function *value* reference (`.map(render)`) matched by name
/// against the file's own definitions. Slightly below [`SAME_FILE_CALL`]: the
/// name match is certain, but a value reference proves only usage, never a
/// call, so it never carries a call's authority. Numerically error-tier, yet
/// outside the tier asserts below — `uses` edges are kind-filtered out of
/// severity decisions entirely.
pub const SAME_FILE_VALUE_REF: f64 = 0.9;

/// Seed confidence for a cross-file call resolved by the path heuristics
/// (import match, same-directory) when the language resolver reports none of
/// its own. Error-tier: a confident structural edge.
pub const CROSS_FILE_HEURISTIC: f64 = 0.80;

/// A resolved BAML boundary call edge. Deliberately warning-tier: BAML
/// resolution is a name-match heuristic across a language boundary, so its
/// edges exist to make the surface visible, never to hard-error a caller.
pub const BAML_BOUNDARY: f64 = 0.75;

/// A call resolved to a symbol exported by another package in the monorepo
/// package index. Warning-tier: the match is a bare-name lookup inside the
/// package's export surface, with no import-graph proof the call reaches it.
pub const CROSS_PACKAGE: f64 = 0.70;

/// A name matched inside framework template markup by a lexical (whole-word)
/// scan — a Svelte `{ ... }` expression naming an imported binding. The markup
/// is never parsed, so the match carries no argument list and no scope
/// analysis: it is evidence of use, never of a call. Deliberately the lowest
/// rung the ladder assigns, and always on a `uses` edge.
pub const TEMPLATE_LEXICAL: f64 = 0.70;

/// A same-file method call on `self`/`this` — the receiver provably denotes the
/// enclosing file's own type, so the method binds with near-certainty.
pub const SELF_RECEIVER_METHOD: f64 = 0.9;

/// A same-file method call on an *unfamiliar* receiver (`obj.m()`), accepted
/// only when exactly one same-file definition carries the name. Warning-tier:
/// the receiver's type is unknown, so this is a uniqueness heuristic.
pub const UNFAMILIAR_RECEIVER_METHOD: f64 = 0.7;

/// Confidence at or above which a resolved call edge is treated as an ERROR
/// source for broken-caller / removed-function checks. Values below it stay
/// warning-tier (dynamic dispatch, cross-language heuristics).
pub const ERROR_TIER_THRESHOLD: f64 = 0.80;

// Every warning-tier edge must stay below the error threshold — that would let
// a heuristic (cross-language name-match, bare-name package export, unknown
// receiver) escalate a caller to a hard ERROR.
// Mirrors the `MIN_DUPLICATE_BODY_LEN` static assert in `keel-enforce`.
const _: () = assert!(BAML_BOUNDARY < ERROR_TIER_THRESHOLD);
const _: () = assert!(CROSS_PACKAGE < ERROR_TIER_THRESHOLD);
const _: () = assert!(UNFAMILIAR_RECEIVER_METHOD < ERROR_TIER_THRESHOLD);
const _: () = assert!(TEMPLATE_LEXICAL < ERROR_TIER_THRESHOLD);
// Error-tier constants must sit at or above it, or the ladder's most certain
// resolutions would silently downgrade.
const _: () = assert!(SAME_FILE_CALL >= ERROR_TIER_THRESHOLD);
const _: () = assert!(SELF_RECEIVER_METHOD >= ERROR_TIER_THRESHOLD);
const _: () = assert!(CROSS_FILE_HEURISTIC >= ERROR_TIER_THRESHOLD);
