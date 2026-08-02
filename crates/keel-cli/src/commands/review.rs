//! `keel review --base <ref>` — the two-sided graph diff, as a PR cover letter.
//!
//! Thin wrapper over the shared [`keel_enforce::review`] core so the CLI and the
//! MCP `keel/review` tool can never disagree about which change leads.

use keel_enforce::review::{self, render};
use keel_output::OutputFormatter;

/// Run `keel review --base <ref>`.
///
/// Never gates: a review that ran is exit 0 whatever it found — the report is
/// the product, and CI decides what to do with it. Only an internal failure (no
/// initialized graph, an unresolvable base ref) exits 2, per the CLI contract.
///
/// Honors the clean-output contract: a diff that moved no contract and touched
/// no file keel cannot parse prints nothing, unless `--verbose` asks for the
/// counts anyway.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, base: String) -> i32 {
    let (cwd, store) = match super::open_store("review") {
        Ok(x) => x,
        Err(code) => return code,
    };

    let result = match review::review(&store, &cwd, &base) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("keel review: {}", e);
            return 2;
        }
    };

    if verbose {
        eprintln!(
            "keel review: {} changed file(s), {} analyzed, {} symbol(s) touched",
            result.files_changed, result.files_analyzed, result.functions_touched,
        );
    }

    let rendered = formatter.format_review(&result);
    if !rendered.is_empty() {
        println!("{}", rendered);
    } else if verbose {
        println!("{}", render::counts_line(&result));
    }

    0
}
