//! Building blocks shared by every part of the engine: errors and limits.

mod error;
mod limits;

pub use error::{Error, ErrorKind, Result};
pub use limits::{Limit, Limits};
