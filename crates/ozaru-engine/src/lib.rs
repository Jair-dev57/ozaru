//! OZARU ENGINE: ozaru's own PDF engine.
//!
//! This crate is in early development and isn't used by the `ozaru` Python package
//! yet. The architecture and roadmap are in `design/engine.md`.
//!
//! [`Document`] is the entry point: it opens a file and gives its pages.

#![forbid(unsafe_code)]

pub mod base;
pub mod content;
pub mod document;
pub mod filters;
pub mod fonts;
pub mod images;
pub mod security;
pub mod syntax;
pub mod text;
pub mod writer;
pub mod xref;

pub use document::{Document, OpenOptions};
