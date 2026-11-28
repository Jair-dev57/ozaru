//! Writing PDF files: objects as PDF syntax, whole documents and incremental
//! updates (ISO 32000-2, sections 7.3 and 7.5).

mod annotation;
mod canvas;
mod editor;
mod encrypt;
mod file;
mod font;
mod forms;
mod image;
mod newfield;
mod redact;
mod serialize;

pub use annotation::{
    AnnotationChanges, AnnotationFilter, AnnotationShape, LinkTarget, MarkupKind, NewAnnotation,
};
pub use canvas::{
    Canvas, LineCap, LineJoin, embedded_placeholder, font_resource, state_resource, text_width,
};
pub use editor::{Editor, FlattenReport, NewAttachment, PageLabel};
pub use encrypt::{Encrypter, Encryption, RANDOM_BYTES};
pub use file::{FileWriter, Layout};
pub use font::EmbeddedFont;
pub use forms::fill_field;
pub use image::{EmbeddedImage, image_resource};
pub use newfield::{NewButton, NewField, NewFieldKind};
pub use redact::{GraphicsMode, ImageMode, RedactArea, RedactOptions, RedactReport};
pub use serialize::{write_dictionary, write_indirect, write_object};
