//! Origami mail core: backend abstraction, sync engine, local store.
//!
//! This crate is UI-agnostic. The Tauri shell (`origami-app`) and the debug
//! CLI (`origami-cli`) both build on top of it. See `docs/PLAN.md`.

pub mod backend;
pub mod config;
pub mod error;
pub mod imap;
pub mod model;
pub mod smtp;

pub use backend::{MailBackend, SmtpSender};
pub use error::{Error, Result};

/// Core crate version, surfaced to the UI for diagnostics.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
