//! Monorepo detection and package enumeration.
//!
//! Detects Cargo workspaces, npm workspaces, Go workspaces, Nx, Turbo, and Lerna
//! monorepos by inspecting config files at the project root.

mod detect;
mod helpers;

use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use detect::*;

/// The kind of monorepo detected at the project root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MonorepoKind {
    CargoWorkspace,
    NpmWorkspaces,
    GoWorkspace,
    NxMonorepo,
    TurboMonorepo,
    LernaMonorepo,
    None,
}

impl Default for MonorepoKind {
    #[allow(clippy::derivable_impls)]
    fn default() -> Self {
        MonorepoKind::None
    }
}

/// Metadata about a single package within a monorepo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub path: PathBuf,
    pub kind: MonorepoKind,
    pub language: String,
}

/// The overall layout of a monorepo: its kind and constituent packages.
#[derive(Debug, Clone, Default)]
pub struct MonorepoLayout {
    pub kind: MonorepoKind,
    pub packages: Vec<PackageInfo>,
}

/// Detect whether `root` is a monorepo and enumerate its packages.
///
/// Tries each detection strategy in priority order and returns the first match.
/// Returns `MonorepoLayout { kind: None, packages: [] }` if nothing matches.
pub fn detect_monorepo(root: &Path) -> MonorepoLayout {
    if let Some(layout) = detect_cargo_workspace(root) {
        return layout;
    }
    if let Some(layout) = detect_npm_workspaces(root) {
        return layout;
    }
    if let Some(layout) = detect_go_workspace(root) {
        return layout;
    }
    if let Some(layout) = detect_nx(root) {
        return layout;
    }
    if let Some(layout) = detect_turbo(root) {
        return layout;
    }
    if let Some(layout) = detect_lerna(root) {
        return layout;
    }
    MonorepoLayout::default()
}

impl std::fmt::Display for MonorepoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonorepoKind::CargoWorkspace => write!(f, "Cargo workspace"),
            MonorepoKind::NpmWorkspaces => write!(f, "npm workspaces"),
            MonorepoKind::GoWorkspace => write!(f, "Go workspace"),
            MonorepoKind::NxMonorepo => write!(f, "Nx monorepo"),
            MonorepoKind::TurboMonorepo => write!(f, "Turbo monorepo"),
            MonorepoKind::LernaMonorepo => write!(f, "Lerna monorepo"),
            MonorepoKind::None => write!(f, "none"),
        }
    }
}

#[cfg(test)]
mod tests;
