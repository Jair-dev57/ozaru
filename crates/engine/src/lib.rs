//! OZARU ENGINE: ozaru's own PDF engine.
//!
//! It reads, draws, edits and protects PDF files, with no other PDF engine
//! underneath, and is the core of the `ozaru` Python package. The architecture and
//! roadmap are in `design/engine.md`.
//!
//! [`Document`] is the entry point: it opens a file and gives its pages.

#![forbid(unsafe_code)]

pub mod base;
pub mod content;
pub mod crypto;
pub mod document;
pub mod filters;
pub mod fonts;
pub mod functions;
pub mod images;
pub mod render;
pub mod security;
pub mod syntax;
pub mod text;
pub mod writer;
pub mod xref;

pub use document::{Document, OpenOptions};
