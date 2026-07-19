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

    let path = std::path::Path::new(&file);
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("keel skeleton: cannot read {}: {}", file, e);
            return 2;
        }
    };

    match keel_enforce::skeleton::build_skeleton(&cwd, path, &content, private, docs) {
        Some(result) => {
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
        None => {
            eprintln!("keel skeleton: unsupported file type: {}", file);
            2
        }
    }
}
