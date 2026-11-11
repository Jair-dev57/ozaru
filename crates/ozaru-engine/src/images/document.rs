use std::collections::HashMap;

use super::description::{ImageDescription, read_image};
use super::export::{ImageFile, image_file};
use crate::base::DiagnosticKind;
use crate::content::{ImageSource, Interpreter};
use crate::document::{Document, Rectangle};
use crate::syntax::ObjectId;
use crate::text::PageProblem;

/// An image painted on a page, as a file if it can be.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    /// Its box on the page, in default user space.
    pub bbox: Rectangle,
    /// The image XObject, or `None` for an inline image.
    pub id: Option<ObjectId>,
    /// What it is. `None` if it can't be read.
    pub description: Option<ImageDescription>,
    /// The image as a file. `None` if it can't be read, or its format or
    /// colors aren't supported yet.
    pub file: Option<ImageFile>,
}

/// The images of one page of a document.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImages {
    pub images: Vec<PageImage>,
    /// What kept some or all of its content from being read, if anything.
    pub problem: Option<PageProblem>,
}

/// The images of every page of `document`, in the order they are painted. An
/// image painted several times is in each place, and is read once.
///
/// A locked document has pages without images: unlock it first.
pub fn document_images(document: &mut Document<'_>) -> Vec<PageImages> {
    let limits = document.limits();
    let pages = document.pages().to_vec();
    let mut interpreter = Interpreter::new(limits);
    let mut read: HashMap<ObjectId, (Option<ImageDescription>, Option<ImageFile>)> = HashMap::new();
    pages
        .iter()
        .map(|page| {
            if page.is_damaged() {
                return PageImages {
                    images: Vec::new(),
                    problem: Some(PageProblem::Damaged),
                };
            }
            let content = interpreter.run_page(document.resolver(), page);
            let unreadable = content
                .diagnostics
                .iter()
                .any(|d| d.kind() == DiagnosticKind::InvalidContents);
            let images = content
                .images
                .iter()
                .map(|placed| {
                    let id = match &placed.source {
                        ImageSource::XObject { id, .. } => Some(*id),
                        ImageSource::Inline { .. } => None,
                    };
                    let cached = id.and_then(|id| read.get(&id).cloned());
                    let (description, file) = cached.unwrap_or_else(|| {
                        let image = read_image(document.resolver(), &placed.source, limits).ok();
                        let file = image.as_ref().and_then(image_file);
                        let result = (image.map(|image| image.description), file);
                        if let Some(id) = id {
                            read.insert(id, result.clone());
                        }
                        result
                    });
                    PageImage {
                        bbox: placed.bounding_box(),
                        id,
                        description,
                        file,
                    }
                })
                .collect();
            PageImages {
                images,
                problem: unreadable.then_some(PageProblem::UnreadableContent),
            }
        })
        .collect()
}
