//! Origami mail core: backend abstraction, sync engine, local store.
//!
//! This crate is UI-agnostic. The Tauri shell (`origami-app`) and the debug
//! CLI (`origami-cli`) both build on top of it. See `docs/PLAN.md`.

pub mod backend;
pub mod blob;
pub mod compose;
pub mod config;
pub mod error;
pub mod imap;
pub mod message;
pub mod model;
pub mod oauth;
mod private_fs;
pub mod prefetch_queue;
pub mod provider_hints;
pub mod smtp;
pub mod store;
pub mod sync;
pub mod threading;

pub use backend::{MailBackend, SmtpSender};
pub use error::{Error, Result};

/// Core crate version, surfaced to the UI for diagnostics.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
