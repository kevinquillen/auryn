//! Session search and filtering.
//!
//! Phase 1 provides case-insensitive substring filtering over session metadata
//! and preview content via [`filter::Filter`]. Phase 6 builds a richer
//! in-memory index on top of these primitives without changing callers.

pub mod filter;
pub mod score;

pub use filter::Filter;
