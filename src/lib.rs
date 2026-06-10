//! Auryn: a local-first, cross-platform browser, searcher, previewer, and
//! resumer for AI coding sessions across multiple providers.
//!
//! The crate is organized so that nothing outside [`providers`] knows how any
//! individual tool stores its sessions, and nothing inside the TUI (added in
//! later phases) contains provider-specific logic. The library is exposed
//! separately from the binary so integration tests can drive it directly.

pub mod app;
pub mod cli;
pub mod config;
pub mod errors;
pub mod format;
pub mod models;
pub mod paths;
pub mod providers;
pub mod search;
pub mod tui;
