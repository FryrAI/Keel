//! `keel skeleton <file>` — a compressed, signature-only view of a file.
//!
//! Purely a parse of the target file (no graph required), so it works before
//! `keel map`. Delegates to `keel_enforce::skeleton::build_skeleton` so the CLI
//! and the `keel/skeleton` MCP tool share one implementation.

use keel_output::OutputFormatter;

/// Run `keel skeleton <file>`.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    file: String,
    docs: bool,
    private: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel skeleton: failed to get current directory: {}", e);
            return 2;
        }
    };

    // The path-resolve → read → parse preamble lives in keel_enforce so the CLI
    // and the `keel/skeleton` MCP tool share one implementation.
    match keel_enforce::skeleton::build_skeleton_from_path(&cwd, &file, private, docs) {
        Ok(result) => {
            if verbose {
                eprintln!(
                    "keel skeleton: {} — {} symbols, {} imports",
                    result.file,
                    result.symbols.len(),
                    result.imports.len(),
                );
            }
            let output = formatter.format_skeleton(&result);
            if !output.is_empty() {
                println!("{}", output.trim_end());
            }
            0
        }
        Err(e) => {
            eprintln!("keel skeleton: {}", e);
            2
        }
    }
}
