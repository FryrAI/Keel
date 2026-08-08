//! Fragment-level (jscpd-style) clone detection over Type-2 token streams
//! (issue #66).
//!
//! W006 compares *whole bodies*: two functions are duplicates or they are not.
//! The commonest real-world copy-paste is smaller than that — a 30-line block
//! lifted into the middle of two otherwise different functions — and no
//! whole-body fingerprint can see it. This module slides a fixed window over
//! the [`crate::hash_t2`] token stream of every function body, hashes each
//! window, and reports how many source lines of each body sit under a window
//! that also occurs in a *different* function.
//!
//! Three deliberate limits:
//!
//! 1. **Cross-function only.** A window repeated inside one body (a `match`
//!    arm pattern, an unrolled loop) is not a clone in the sense that matters;
//!    only a window owned by two different functions counts.
//! 2. **Counts, never locations.** The output is a line *count* per function.
//!    A matched window is a token shape, not necessarily code a human would
//!    call duplicated — on keel's own tree the tail of the list is dispatch
//!    boilerplate — so the aggregate is reported as a trend metric and never
//!    as a violation with a line number attached.
//! 3. **Batch, never incremental.** The scan needs every body at once, so it
//!    runs inside `keel map` and its result is stored per function. `keel
//!    compile` neither reads nor refreshes it — the same bounded staleness the
//!    rest of the map-time indexes carry, and the reason fragment scanning
//!    costs the compile hot path nothing.
//!
//! The window hash is Rabin-Karp over interned token ids, so the whole scan is
//! O(total tokens) rather than O(tokens · window).
//!
//! **Why the windows are not Type-2-renamed.** `hash_t2`'s positional renaming
//! (`v0`, `v1`, … in first-appearance order) is defined over a *whole body*:
//! the same copied block sitting after a different prefix in two functions gets
//! different numbers, so its two token streams do not match — a fragment
//! detector built on the renamed stream finds only clones that are already
//! whole-body clones, which W006 reports anyway. Windows are therefore
//! tokenized with [`hash_t2::IdentifierMode::Verbatim`]: comments dropped,
//! literals collapsed to `<int>`/`<str>`/`<bool>`, formatting invisible,
//! identifiers kept. Renamed copies are the price; catching them at fragment
//! level needs Baker's parameterized matching, which a rolling hash cannot do.

use std::collections::HashMap;

use crate::hash_t2;
use crate::types::FragmentCloneEntry;

/// Window length, in Type-2 tokens, that a repeated fragment must reach.
///
/// Calibrated by mapping keel's own tree (issue #66). The window is the only
/// knob that separates "copied" from "shaped alike", and the ratio it produces
/// is steeply sensitive to it:
///
/// | window | functions with a clone | `clone_loc_ratio` |
/// |-------:|----------------------:|------------------:|
/// |     30 |                   311 |              0.18 |
/// |     40 |                   196 |              0.12 |
/// |     50 |                   140 |              0.09 |
/// |     70 |                    71 |              0.05 |
/// |    100 |                    34 |              0.03 |
///
/// At 30 the additions are shape coincidences — two-line helpers like
/// `param_str`/`param_str_opt`, `parse`/`parse_file`, any pair of small
/// `match` dispatchers — and the reading moves with idiom rather than with
/// copying. At 50 the head of the list is unambiguous: `check_missing_docstring`
/// against `check_missing_type_hints`, `insert_node` against
/// `update_node_in_db`, and every `format_*` pair that keel-output renders
/// twice (once for humans, once for the LLM). Above 70 real duplicated
/// machinery starts dropping out. 50 tokens is also jscpd's own default.
pub const FRAGMENT_WINDOW_TOKENS: usize = 50;

/// Multiplier for the Rabin-Karp rolling hash. Odd, so the recurrence stays
/// invertible mod 2^64.
const HASH_BASE: u64 = 0x100_0000_01b3;

/// Mixer applied to every interned token id before it enters the hash.
///
/// Token ids are small, dense integers; feeding them raw into a `h * B + id`
/// recurrence makes the low bits of nearby windows correlate. Multiplying by
/// the golden-ratio constant first spreads them across the whole word.
const ID_MIX: u64 = 0x9E37_79B9_7F4A_7C15;

/// One function body, reduced to what the scan needs.
struct Unit {
    /// Identity of the function node, carried through to the stored row.
    node_hash: String,
    name: String,
    file_path: String,
    line: u32,
    /// Interned Type-2 token ids, in body order.
    ids: Vec<u32>,
    /// Body-relative line of each token, non-decreasing.
    lines: Vec<u32>,
}

/// Accumulates function bodies during `keel map`, then reports per-function
/// cloned-line counts.
///
/// Memory is bounded by the total token count of the repo: two `u32`s per
/// token, plus one `String` per *distinct* token. keel's own tree interns
/// ~640k tokens over ~24k distinct ones — a few megabytes, held only for the
/// duration of one `keel map`.
#[derive(Default)]
pub struct FragmentScan {
    /// Token text → dense id. Interning makes window hashing integer work.
    vocab: HashMap<std::borrow::Cow<'static, str>, u32>,
    units: Vec<Unit>,
}

impl FragmentScan {
    /// An empty scan.
    pub fn new() -> Self {
        FragmentScan::default()
    }

    /// Add one function body to the scan.
    ///
    /// `tokens` must be the body's [`hash_t2::IdentifierMode::Verbatim`]
    /// stream — see the module docs for why the renamed one cannot work here.
    /// The caller passes it in rather than a body plus a language because
    /// `keel map` derives the Type-2 whole-body fingerprint from the very same
    /// vector, so the two measurements cannot tokenize differently. Callers
    /// also decide which files are worth scanning; this type applies no
    /// file-class rule of its own.
    pub fn add(
        &mut self,
        node_hash: String,
        name: String,
        file_path: String,
        line: u32,
        tokens: Vec<hash_t2::PositionedToken>,
    ) {
        if tokens.is_empty() {
            return;
        }
        let mut ids = Vec::with_capacity(tokens.len());
        let mut lines = Vec::with_capacity(tokens.len());
        for token in tokens {
            let next = self.vocab.len() as u32;
            ids.push(*self.vocab.entry(token.text).or_insert(next));
            lines.push(token.line);
        }
        self.units.push(Unit {
            node_hash,
            name,
            file_path,
            line,
            ids,
            lines,
        });
    }

    /// Score every added body: one row per function, cloned lines and total
    /// code lines.
    ///
    /// Two passes over the same window hashes. The first records which
    /// function owns each window and flags every window claimed by two
    /// different functions; the second re-walks each body and marks the token
    /// positions covered by a flagged window. A row is emitted for every
    /// function — including the ones with no clone at all — because the metric
    /// built on this is a *ratio*, and its denominator is exactly the code
    /// lines counted here.
    pub fn finish(self) -> Vec<FragmentCloneEntry> {
        let hashes: Vec<Vec<u64>> = self
            .units
            .iter()
            .map(|u| window_hashes(&u.ids, FRAGMENT_WINDOW_TOKENS))
            .collect();

        // window hash -> the single unit that owns it, or `SHARED`.
        const SHARED: usize = usize::MAX;
        let mut owner: HashMap<u64, usize> = HashMap::new();
        for (index, unit_hashes) in hashes.iter().enumerate() {
            for h in unit_hashes {
                owner
                    .entry(*h)
                    .and_modify(|o| {
                        if *o != index {
                            *o = SHARED;
                        }
                    })
                    .or_insert(index);
            }
        }

        self.units
            .iter()
            .zip(&hashes)
            .map(|(unit, unit_hashes)| {
                let mut covered = vec![false; unit.ids.len()];
                // Windows are visited in increasing start position, so
                // `filled` lets overlapping matches share work: the whole
                // marking pass stays O(tokens) instead of O(tokens · window).
                let mut filled = 0usize;
                for (start, h) in unit_hashes.iter().enumerate() {
                    if owner.get(h) != Some(&SHARED) {
                        continue;
                    }
                    let end = start + FRAGMENT_WINDOW_TOKENS;
                    for slot in covered.iter_mut().take(end).skip(filled.max(start)) {
                        *slot = true;
                    }
                    filled = end;
                }
                FragmentCloneEntry {
                    node_hash: unit.node_hash.clone(),
                    name: unit.name.clone(),
                    file_path: unit.file_path.clone(),
                    line: unit.line,
                    cloned_lines: distinct_lines(&unit.lines, Some(&covered)),
                    code_lines: distinct_lines(&unit.lines, None),
                }
            })
            .collect()
    }
}

/// How many distinct lines the selected token positions touch.
///
/// `lines` is non-decreasing, so a distinct count is one linear scan with no
/// set. `selected` restricts the count to marked positions; `None` counts them
/// all.
fn distinct_lines(lines: &[u32], selected: Option<&[bool]>) -> u32 {
    let mut count = 0u32;
    let mut previous: Option<u32> = None;
    for (i, line) in lines.iter().enumerate() {
        if selected.is_some_and(|s| !s[i]) {
            continue;
        }
        if previous != Some(*line) {
            count += 1;
            previous = Some(*line);
        }
    }
    count
}

/// Rabin-Karp hashes of every `window`-token span of `ids`, in start order.
///
/// Empty when the stream is shorter than one window. Collisions are possible
/// in principle — the hash is 64 bits and never verified against the tokens —
/// but at the ~10^6 windows a large repo produces, the expected number of
/// false pairs is ~10^-7, well below the noise the window length itself
/// carries.
fn window_hashes(ids: &[u32], window: usize) -> Vec<u64> {
    if window == 0 || ids.len() < window {
        return Vec::new();
    }
    let mut top = 1u64;
    for _ in 1..window {
        top = top.wrapping_mul(HASH_BASE);
    }

    let mut hash = 0u64;
    for id in &ids[..window] {
        hash = hash.wrapping_mul(HASH_BASE).wrapping_add(mix(*id));
    }

    let mut out = Vec::with_capacity(ids.len() - window + 1);
    out.push(hash);
    for i in window..ids.len() {
        hash = hash
            .wrapping_sub(mix(ids[i - window]).wrapping_mul(top))
            .wrapping_mul(HASH_BASE)
            .wrapping_add(mix(ids[i]));
        out.push(hash);
    }
    out
}

/// Spread a dense token id across the whole 64-bit word.
fn mix(id: u32) -> u64 {
    (id as u64).wrapping_add(1).wrapping_mul(ID_MIX)
}

#[cfg(test)]
#[path = "fragments_tests.rs"]
mod tests;
