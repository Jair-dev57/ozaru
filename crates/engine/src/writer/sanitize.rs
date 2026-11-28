//! Sanitizing: taking out of a document what it carries besides what its
//! pages show, and leaving the pages as they are.

/// What a sanitized document loses. The options with `true` by default take out
/// what doesn't change how the pages look or work; the others change the
/// document and are asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// The information dictionary (title, author, dates, creator...) except the
    /// entries named in `keep_info`, the XMP metadata, the private data of
    /// applications (`/PieceInfo`), thumbnails, and the metadata of pages,
    /// images and drawings.
    pub metadata: bool,
    /// Entries of the information dictionary to keep, such as `Title`.
    pub keep_info: Vec<String>,
    /// JavaScript and the other actions that run things (launching files,
    /// sending forms, playing media), the additional actions of the document,
    /// pages, annotations and fields, XFA forms, and the annotations that play
    /// media. Links to addresses and pages stay.
    pub scripts: bool,
    /// Attached files: those of the document, the annotations that hold a file,
    /// and associated files.
    pub attachments: bool,
    /// The bookmarks.
    pub bookmarks: bool,
    /// The comments: annotations that aren't links or form fields (notes,
    /// highlights, shapes, stamps...).
    pub comments: bool,
    /// The values of form fields, which are emptied (and drawn empty).
    pub form_values: bool,
    /// The structure of tagged files: tags, alternative text and the reading
    /// order for assistive software.
    pub structure: bool,
}

impl Default for SanitizeOptions {
    fn default() -> Self {
        Self {
            metadata: true,
            keep_info: Vec::new(),
            scripts: true,
            attachments: true,
            bookmarks: false,
            comments: false,
            form_values: false,
            structure: false,
        }
    }
}

impl SanitizeOptions {
    /// Everything taken out, as far as it goes.
    pub fn everything() -> Self {
        Self {
            bookmarks: true,
            comments: true,
            form_values: true,
            structure: true,
            ..Self::default()
        }
    }
}

/// What a sanitizing took out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SanitizeReport {
    /// Entries of the information dictionary.
    pub info_entries: usize,
    /// Metadata streams (XMP) of the document, pages, images and drawings.
    pub metadata_streams: usize,
    /// Private data of applications and thumbnails.
    pub private_data: usize,
    /// Actions, additional actions, XFA forms and media annotations.
    pub scripts: usize,
    /// Attached files and the annotations that held them.
    pub attachments: usize,
    pub bookmarks: usize,
    pub comments: usize,
    /// Fields whose value was emptied.
    pub form_values: usize,
    /// Structure of the document and of its pages.
    pub structure: usize,
}

impl SanitizeReport {
    /// Whether anything was taken out.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
