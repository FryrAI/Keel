//! `keel review --base <ref>` — the two-sided graph diff, as a PR cover letter.
//!
//! Thin wrapper over the shared [`keel_enforce::review`] core so the CLI and the
//! MCP `keel/review` tool can never disagree about which change leads.

use keel_enforce::review::{self, baseline, render};
use keel_output::OutputFormatter;

/// Run `keel review --base <ref>`.
///
/// Exit 0 whatever it found, unless `--gate` is set *and* `review.gate` in
/// `keel.json` names a code the diff introduced: the report is the product, and
/// a repo opts into a red build per code, once, in config. Only an internal
/// failure (no initialized graph, an unresolvable base ref) exits 2, per the
/// CLI contract.
///
/// Honors the clean-output contract: a diff that moved no contract, introduced
/// no violation, and touched no file keel cannot parse prints nothing, unless
/// `--verbose` asks for the counts anyway.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    base: String,
    github: bool,
    gate: bool,
) -> i32 {
    let ctx = match super::open_repo("review") {
        Ok(x) => x,
        Err(code) => return code,
    };
    let config = keel_core::config::KeelConfig::load(&ctx.keel_dir);
    let (cwd, store) = (ctx.cwd, ctx.store);

    // Detect (never rewrite — Principle 7) a binary/docs version mismatch, as
    // `map` and `compile` do. Review is the only keel command a CI run makes
    // when the graph came from cache, so without this line the drift notice
    // never reaches the one reader who can act on it.
    super::version_drift::warn(&cwd, &config);

    let result = match review::review(&store, &cwd, &base, &config.enforce) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("keel review: {}", e);
            return 2;
        }
    };

    if verbose {
        eprintln!(
            "keel review: {} changed file(s), {} analyzed, {} symbol(s) touched, \
             {} new violation(s), {} pre-existing",
            result.files_changed,
            result.files_analyzed,
            result.functions_touched,
            result.new_violations.len(),
            result.pre_existing_violations,
        );
    }

    // `--format github` is a violation protocol: it annotates what the diff
    // introduced and stays silent about contract changes, which have no line to
    // anchor an annotation to.
    let rendered = if github {
        keel_output::github::annotations(&result.new_violations)
    } else {
        formatter.format_review(&result)
    };
    if !rendered.is_empty() {
        println!("{}", rendered);
    } else if verbose && !github {
        println!("{}", render::counts_line(&result));
    }

    let hits = if gate {
        baseline::gate_hits(&result.new_violations, &config.review.gate)
    } else {
        Vec::new()
    };
    if !hits.is_empty() {
        eprintln!(
            "keel review: {} new violation(s) in gated codes ({})",
            hits.len(),
            config.review.gate.join(", "),
        );
        return 1;
    }
    if gate && config.review.gate.is_empty() && verbose {
        eprintln!("keel review: --gate set but review.gate in keel.json is empty; gating nothing");
    }
    0
}
