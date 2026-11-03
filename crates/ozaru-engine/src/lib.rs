//! OZARU ENGINE: ozaru's own PDF engine.
//!
//! This crate is in early development and isn't used by the `ozaru` Python package
//! yet. The architecture and roadmap are in `design/engine.md`.

#![forbid(unsafe_code)]

pub mod base;
pub mod syntax;
pub mod xref;
