//! Core types, graph storage, and configuration for keel.
//!
//! This crate provides the foundational data structures used across all keel crates:
//! - [`types`] — Graph nodes, edges, and error types
//! - [`store`] — The [`GraphStore`](store::GraphStore) trait for graph persistence
//! - [`sqlite`] — SQLite-backed implementation of `GraphStore`
//! - [`config`] — Configuration loading from `.keel/keel.json`
//! - [`hash`] — Deterministic content hashing (base62 of xxhash64)
//! - [`telemetry`] — Privacy-safe telemetry storage

pub mod config;
pub mod hash;
pub mod sqlite;
pub mod sqlite_batch;
pub mod sqlite_helpers;
pub mod sqlite_queries;
pub mod store;
pub mod telemetry;
pub mod types;
