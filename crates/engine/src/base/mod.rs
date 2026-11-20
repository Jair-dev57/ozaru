//! Building blocks shared by every part of the engine: errors, diagnostics and
//! limits.

mod diagnostics;
mod error;
mod limits;

pub use diagnostics::{Diagnostic, DiagnosticKind, Diagnostics};
pub use error::{Error, ErrorKind, Result};
pub use limits::{Limit, Limits};
