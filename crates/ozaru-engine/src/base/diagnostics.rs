use std::fmt;

/// A recoverable problem: the engine worked around it and kept going.
///
/// Unlike an [`Error`](super::Error), a diagnostic doesn't stop the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    kind: DiagnosticKind,
    offset: usize,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    pub fn kind(&self) -> DiagnosticKind {
        self.kind
    }

    /// Position in the input, in bytes from the start, where the problem was found.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.offset)
    }
}

/// What the engine worked around. New kinds are added as the engine grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// A dictionary has the same key more than once. The last value is kept.
    DuplicateDictionaryKey,
    /// An indirect object has no `endobj`. The object is read anyway.
    MissingEndobj,
    /// A stream has no `/Length`. Its data ends at `endstream`.
    MissingStreamLength,
    /// A stream's `/Length` isn't a valid integer, or doesn't end where
    /// `endstream` is. The data ends at `endstream`.
    WrongStreamLength,
    /// A stream's `/Length` is a reference that wasn't resolved: it was parsed
    /// without a resolver, or the reference doesn't lead to an integer. The data
    /// ends at `endstream`.
    UnresolvedStreamLength,
    /// The `stream` keyword is followed by a lone carriage return instead of
    /// CR LF or LF.
    StreamLineEndIsCarriageReturn,
    /// The `stream` keyword isn't followed by an end of line. The data starts right
    /// after it.
    MissingStreamLineEnd,
    /// The file doesn't end with `%%EOF` after the `startxref` offset.
    MissingEndOfFileMarker,
    /// A cross-reference entry isn't exactly 20 bytes: 10 digits, a space, 5
    /// digits, a space, `n` or `f` and a two-byte end of line. It is read anyway,
    /// and only the first one in a table is reported.
    NonStandardXrefEntry,
    /// A cross-reference subsection has fewer entries than its header says. The
    /// ones present are kept.
    XrefSubsectionTooShort,
    /// A cross-reference table lists the same object number twice. The first
    /// entry is kept.
    DuplicateXrefEntry,
    /// Flate data is cut short, damaged or has a wrong checksum. The data decoded
    /// before the problem is kept.
    DamagedFlateData,
    /// Flate data has no zlib header. It is decoded as raw deflate data.
    MissingZlibHeader,
    /// The last row of predicted data is shorter than the others. It is decoded
    /// as far as it goes.
    IncompletePredictorRow,
    /// A cross-reference stream has less data than its `/Index` needs. The
    /// complete entries are kept.
    XrefStreamTooShort,
    /// A cross-reference stream has an entry of an unknown type. It is ignored,
    /// as the standard says, and only the first one in a stream is reported.
    UnknownXrefEntryType,
    /// A `/Prev` points back at a cross-reference section already read. The chain
    /// stops there.
    XrefLoop,
    /// An older cross-reference section, reached through `/Prev` or `/XRefStm`,
    /// can't be read. The entries of the newer sections are kept.
    BrokenXrefSection,
    /// An object stream's header lists fewer objects than `/N`. The ones listed
    /// are kept.
    ObjectStreamHeaderTooShort,
    /// A reference has another generation than the cross-reference entry of its
    /// object. As the standard says, it resolves to null.
    GenerationMismatch,
    /// An object isn't at the index its cross-reference entry gives in its object
    /// stream. It was found by its number.
    WrongObjectStreamIndex,
    /// The cross-reference data can't be read, so it was rebuilt by scanning the
    /// file for objects. Reported where the original data failed.
    XrefRebuilt,
    /// An object isn't where the cross-reference data says, but scanning the file
    /// found it elsewhere. Reported at the offset that was wrong.
    ObjectRelocated,
    /// Filter data has no end marker: `>` or `~>` in ASCIIHex and ASCII85 data,
    /// 128 in RunLength data or code 257 in LZW data. It is decoded to its end.
    MissingEndOfDataMarker,
    /// Filter data ends in the middle of a run. The bytes it has are kept.
    TruncatedFilterData,
    /// AES-encrypted data isn't a whole number of 16-byte blocks after its
    /// initialization vector, or its padding is wrong. It is decrypted as far as
    /// it goes.
    DamagedEncryptedData,
    /// The catalog has no `/Type /Catalog`, or another type, but has `/Pages`. It
    /// is used as the catalog.
    MissingCatalogType,
    /// The trailer's `/Root` is missing or isn't a usable catalog, so another
    /// object of type `/Catalog` was used. Reported at that object.
    CatalogFound,
    /// No catalog has a usable `/Pages`, so the root of the page tree was found
    /// among the objects: one of `/Type /Pages` without `/Parent`. Reported at
    /// that object.
    PageTreeFound,
    /// A node of the page tree appears again inside itself, or a page appears
    /// twice. The repeated one is skipped.
    PageTreeCycle,
    /// A kid of the page tree is null, isn't a dictionary, or is neither a page
    /// nor a node with `/Kids`. It is skipped.
    InvalidPageTreeNode,
    /// A kid of the page tree exists but can't be read. It is kept as a damaged
    /// page.
    DamagedPage,
    /// The `/Count` of the root of the page tree isn't the number of pages found.
    /// The pages found are used.
    WrongPageCount,
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDictionaryKey => {
                f.write_str("repeated dictionary key, the last value is kept")
            }
            Self::MissingEndobj => f.write_str("missing endobj, the object is read anyway"),
            Self::MissingStreamLength => {
                f.write_str("stream has no length, its data ends at endstream")
            }
            Self::WrongStreamLength => {
                f.write_str("stream length is wrong, its data ends at endstream")
            }
            Self::UnresolvedStreamLength => {
                f.write_str("stream length is a reference, its data ends at endstream")
            }
            Self::StreamLineEndIsCarriageReturn => {
                f.write_str("stream keyword is followed by a lone carriage return")
            }
            Self::MissingStreamLineEnd => {
                f.write_str("stream keyword is not followed by an end of line")
            }
            Self::MissingEndOfFileMarker => f.write_str("missing %%EOF after startxref"),
            Self::NonStandardXrefEntry => {
                f.write_str("cross-reference entry is not 20 bytes, it is read anyway")
            }
            Self::XrefSubsectionTooShort => {
                f.write_str("cross-reference subsection has fewer entries than stated")
            }
            Self::DuplicateXrefEntry => {
                f.write_str("repeated cross-reference entry, the first one is kept")
            }
            Self::DamagedFlateData => {
                f.write_str("Flate data is damaged, the data before the damage is kept")
            }
            Self::MissingZlibHeader => {
                f.write_str("Flate data has no zlib header, it is read as raw deflate")
            }
            Self::IncompletePredictorRow => {
                f.write_str("last predictor row is incomplete, it is decoded as far as it goes")
            }
            Self::XrefStreamTooShort => {
                f.write_str("cross-reference stream has fewer entries than its index")
            }
            Self::UnknownXrefEntryType => {
                f.write_str("cross-reference stream entry of unknown type, it is ignored")
            }
            Self::XrefLoop => {
                f.write_str("cross-reference sections point at each other, the chain stops")
            }
            Self::BrokenXrefSection => {
                f.write_str("older cross-reference section is broken, newer entries are kept")
            }
            Self::ObjectStreamHeaderTooShort => {
                f.write_str("object stream lists fewer objects than stated")
            }
            Self::GenerationMismatch => {
                f.write_str("reference generation doesn't match, it resolves to null")
            }
            Self::WrongObjectStreamIndex => {
                f.write_str("object is not at its index in the object stream, found by number")
            }
            Self::XrefRebuilt => {
                f.write_str("cross-reference data is broken, it was rebuilt by scanning the file")
            }
            Self::ObjectRelocated => {
                f.write_str("object is not where the cross-reference data says, found by scanning")
            }
            Self::MissingEndOfDataMarker => {
                f.write_str("filter data has no end marker, it is decoded to its end")
            }
            Self::TruncatedFilterData => {
                f.write_str("filter data ends in the middle of a run, the bytes it has are kept")
            }
            Self::DamagedEncryptedData => {
                f.write_str("encrypted data is damaged, it is decrypted as far as it goes")
            }
            Self::MissingCatalogType => {
                f.write_str("catalog has no /Type /Catalog, it is used because it has /Pages")
            }
            Self::CatalogFound => {
                f.write_str("trailer has no usable /Root, another catalog object was used")
            }
            Self::PageTreeFound => {
                f.write_str("no catalog has usable /Pages, the root of the page tree was found")
            }
            Self::PageTreeCycle => {
                f.write_str("page tree node or page repeated, the repetition is skipped")
            }
            Self::InvalidPageTreeNode => {
                f.write_str("page tree kid is not a page or a node, it is skipped")
            }
            Self::DamagedPage => {
                f.write_str("page tree kid can't be read, it is kept as a damaged page")
            }
            Self::WrongPageCount => {
                f.write_str("page tree /Count is wrong, the pages found are used")
            }
        }
    }
}

/// The diagnostics collected during an operation.
///
/// It keeps at most `capacity` of them, so a hostile file can't fill the memory
/// with warnings. The rest are only counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
    capacity: usize,
    dropped: usize,
}

impl Diagnostics {
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Vec::new(),
            capacity,
            dropped: 0,
        }
    }

    pub fn push(&mut self, kind: DiagnosticKind, offset: usize) {
        if self.items.len() < self.capacity {
            self.items.push(Diagnostic::new(kind, offset));
        } else {
            self.dropped += 1;
        }
    }

    /// Adds the diagnostics of `other` after these, keeping the capacity. Those of
    /// `other` that weren't kept are counted as dropped here too.
    pub fn append(&mut self, other: &Diagnostics) {
        for diagnostic in other.iter() {
            self.push(diagnostic.kind(), diagnostic.offset());
        }
        self.dropped += other.dropped;
    }

    /// Number of diagnostics kept.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` if nothing was reported, not even diagnostics that weren't kept.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.dropped == 0
    }

    /// Number of diagnostics that were reported after the capacity was reached.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }
}
