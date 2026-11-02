use std::ops::Range;

use super::Dictionary;

/// A PDF stream: a dictionary followed by a sequence of bytes, such as the content
/// of a page or an image (ISO 32000-2, section 7.3.8).
///
/// It doesn't copy the bytes: it records where they are in the file, so large
/// streams don't take memory twice. They are decoded only when needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    dictionary: Dictionary,
    data: Range<usize>,
}

impl Stream {
    /// `data` is the range of the stream's raw bytes in the file.
    pub fn new(dictionary: Dictionary, data: Range<usize>) -> Self {
        Self { dictionary, data }
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Where the raw, undecoded bytes are in the file.
    pub fn data_range(&self) -> Range<usize> {
        self.data.clone()
    }

    /// The raw, undecoded bytes, taken from `file`. `None` if the range doesn't fit
    /// in `file`, for example when given a different file.
    pub fn raw_data<'a>(&self, file: &'a [u8]) -> Option<&'a [u8]> {
        file.get(self.data.clone())
    }
}
