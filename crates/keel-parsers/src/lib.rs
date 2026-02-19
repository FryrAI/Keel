//! Language-specific parsers and resolution engine for keel.
//!
//! Implements the 3-tier hybrid resolution strategy:
//! - **Tier 1:** tree-sitter for universal structural extraction
//! - **Tier 2:** Per-language enhancers (oxc for TS, ty for Python, heuristics for Go/Rust)
//! - **Tier 3:** LSP/SCIP on-demand (optional, >95% accuracy)
//!
//! Supported languages: TypeScript/JavaScript, Python, Go, Rust.

pub mod queries;
pub mod resolver;
pub mod treesitter;
pub mod walker;

pub mod typescript;
pub mod python;
pub mod go;
pub mod rust_lang;
