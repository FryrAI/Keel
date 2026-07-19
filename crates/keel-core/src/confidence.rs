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

/// Seed confidence for a cross-file call resolved by the path heuristics
/// (import match, same-directory) when the language resolver reports none of
/// its own. Error-tier: a confident structural edge.
pub const CROSS_FILE_HEURISTIC: f64 = 0.80;

/// The last-resort bare-name graph lookup in the compile sync: the callee's
/// final segment matched a single unambiguous graph node. Error-tier.
pub const BARE_NAME_FALLBACK: f64 = 0.80;

/// A resolved BAML boundary call edge. Deliberately warning-tier: BAML
/// resolution is a name-match heuristic across a language boundary, so its
/// edges exist to make the surface visible, never to hard-error a caller.
pub const BAML_BOUNDARY: f64 = 0.75;

/// Confidence at or above which a resolved call edge is treated as an ERROR
/// source for broken-caller / removed-function checks. Values below it stay
/// warning-tier (dynamic dispatch, cross-language heuristics).
pub const ERROR_TIER_THRESHOLD: f64 = 0.80;

// A warning-tier edge must never sit at or above the error threshold — that
// would let a cross-language name-match escalate a caller to a hard ERROR.
// Mirrors the `MIN_DUPLICATE_BODY_LEN` static assert in `keel-enforce`.
const _: () = assert!(BAML_BOUNDARY < ERROR_TIER_THRESHOLD);
