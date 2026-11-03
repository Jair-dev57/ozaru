//! Stream filters: how the data of a stream is decoded (ISO 32000-2, section 7.4).
//!
//! Phase 3 only needs Flate and the PNG predictors, which cross-reference streams
//! and object streams use. The other filters come in phase 4.

mod flate;
mod predictor;

pub use flate::decode_flate;

use crate::base::Diagnostics;

/// Data decoded by a filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub data: Vec<u8>,
    /// Problems found and worked around while decoding it.
    pub diagnostics: Diagnostics,
}
