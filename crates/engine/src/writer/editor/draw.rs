//! Drawing on pages that exist: the content of a [`Canvas`] added to a page
//! without changing what the page has.

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::filters::deflate_zlib;
use crate::fonts::StandardFont;
use crate::syntax::{Dictionary, Name, Object, ObjectId};
use crate::writer::canvas::{Canvas, embedded_placeholder, font_resource, state_resource};
use crate::writer::image::image_resource;

impl Editor<'_, '_> {
    /// Draws `canvas` on page `index`, over what the page has.
    ///
    /// The page's own content is wrapped in a saved graphics state, so what
    /// it leaves changed, such as a matrix, doesn't move the drawing. The
    /// fonts and graphics states the canvas uses are added to the page's
    /// resources, which are copied from its tree if it inherited them.
    ///
    /// It is a `PageNotFound` error past the last page.
    pub fn draw(&mut self, index: usize, canvas: &Canvas) -> Result<()> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        if canvas.is_empty() {
            return Ok(());
        }
        let id = self.page_object(index)?;
        let mut page = self.page_dictionary(id)?;

        let mut resources = self.page_resources(index, &page)?;
        // The content, with the fonts of the user under the names they get.
        let mut content = canvas.content().to_vec();
        if !canvas.fonts().is_empty() || !canvas.embedded_fonts().is_empty() {
            let mut fonts = self.sub_dictionary(&resources, "Font")?;
            for &font in canvas.fonts() {
                let object = self.add(Object::Dictionary(font_dictionary(font)));
                fonts.insert(
                    Name::from(font_resource(font).as_str()),
                    Object::Reference(object),
                );
            }
            for (position, (font, glyphs)) in canvas.embedded_fonts().iter().enumerate() {
                let name = font.resource_name(glyphs);
                let dictionary = font.embed(glyphs, self)?;
                let object = self.add(Object::Dictionary(dictionary));
                fonts.insert(Name::from(name.as_str()), Object::Reference(object));
                content = replace_name(&content, &embedded_placeholder(position), &name);
            }
            resources.insert(Name::from("Font"), Object::Dictionary(fonts));
        }
        if !canvas.states().is_empty() {
            let mut states = self.sub_dictionary(&resources, "ExtGState")?;
            for &(fill, stroke) in canvas.states() {
                let mut state = Dictionary::new();
                state.insert(Name::from("Type"), Object::Name(Name::from("ExtGState")));
                state.insert(Name::from("ca"), Object::Real(fill));
                state.insert(Name::from("CA"), Object::Real(stroke));
                states.insert(
                    Name::from(state_resource(fill, stroke).as_str()),
                    Object::Dictionary(state),
                );
            }
            resources.insert(Name::from("ExtGState"), Object::Dictionary(states));
        }

        if !canvas.images().is_empty() {
            let mut objects = self.sub_dictionary(&resources, "XObject")?;
            for image in canvas.images() {
                let object = image.embed(self);
                objects.insert(
                    Name::from(image_resource(image).as_str()),
                    Object::Reference(object),
                );
            }
            resources.insert(Name::from("XObject"), Object::Dictionary(objects));
        }

        // The page's own content between a `q` and a `Q`, then the drawing.
        let mut contents = vec![Object::Reference(self.add_content(b"q\n"))];
        contents.extend(self.content_streams(&page)?);
        contents.push(Object::Reference(self.add_content(b"Q\n")));
        contents.push(Object::Reference(self.add_content(&content)));

        page.insert(Name::from("Resources"), Object::Dictionary(resources));
        page.insert(Name::from("Contents"), Object::Array(contents));
        self.set(id, Object::Dictionary(page));
        Ok(())
    }

    /// Appends `content` to page `index`, over what the page has, with `xobjects`
    /// (names and objects) added to the page's resources. The page's own content
    /// is wrapped in a saved graphics state first, as [`draw`](Self::draw) does.
    pub(super) fn append_content(
        &mut self,
        index: usize,
        content: &[u8],
        xobjects: &[(Name, ObjectId)],
    ) -> Result<()> {
        let id = self.page_object(index)?;
        let mut page = self.page_dictionary(id)?;
        let mut resources = self.page_resources(index, &page)?;
        if !xobjects.is_empty() {
            let mut objects = self.sub_dictionary(&resources, "XObject")?;
            for (name, object) in xobjects {
                objects.insert(name.clone(), Object::Reference(*object));
            }
            resources.insert(Name::from("XObject"), Object::Dictionary(objects));
        }
        let mut contents = vec![Object::Reference(self.add_content(b"q\n"))];
        contents.extend(self.content_streams(&page)?);
        contents.push(Object::Reference(self.add_content(b"Q\n")));
        contents.push(Object::Reference(self.add_content(content)));
        page.insert(Name::from("Resources"), Object::Dictionary(resources));
        page.insert(Name::from("Contents"), Object::Array(contents));
        self.set(id, Object::Dictionary(page));
        Ok(())
    }

    // A compressed content stream with `data`.
    pub(super) fn add_content(&mut self, data: &[u8]) -> ObjectId {
        let mut dictionary = Dictionary::new();
        dictionary.insert(
            Name::from("Filter"),
            Object::Name(Name::from("FlateDecode")),
        );
        self.add_stream(dictionary, deflate_zlib(data, 6))
    }

    // The resources of page `index` as a dictionary of its own: what the page
    // has, or what it inherits.
    pub(super) fn page_resources(&mut self, index: usize, page: &Dictionary) -> Result<Dictionary> {
        let entry = match page.get("Resources") {
            Some(entry) => Some(entry.clone()),
            None => self.pages[index]
                .source
                .and_then(|source| self.document.page(source))
                .and_then(|page| page.attribute("Resources"))
                .cloned(),
        };
        self.dictionary_of(entry)
    }

    // The dictionary an entry stands for, directly or by reference; empty if
    // it isn't one.
    pub(super) fn dictionary_of(&mut self, entry: Option<Object>) -> Result<Dictionary> {
        Ok(match entry {
            Some(Object::Dictionary(dictionary)) => dictionary,
            Some(Object::Reference(id)) => self
                .object(id)?
                .as_dictionary()
                .cloned()
                .unwrap_or_default(),
            _ => Dictionary::new(),
        })
    }

    // The dictionary `resources` has under `key`, as a dictionary of its own.
    pub(super) fn sub_dictionary(
        &mut self,
        resources: &Dictionary,
        key: &str,
    ) -> Result<Dictionary> {
        self.dictionary_of(resources.get(key).cloned())
    }

    // The streams a page's `/Contents` names, as references.
    pub(super) fn content_streams(&mut self, page: &Dictionary) -> Result<Vec<Object>> {
        Ok(match page.get("Contents") {
            Some(Object::Reference(id)) => match self.object(*id)? {
                Object::Array(items) => items,
                _ => vec![Object::Reference(*id)],
            },
            Some(Object::Array(items)) => items.clone(),
            _ => Vec::new(),
        })
    }
}

// The font dictionary of a standard font: a name is all a reader needs.
pub(super) fn font_dictionary(font: StandardFont) -> Dictionary {
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("Type"), Object::Name(Name::from("Font")));
    dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Type1")));
    dictionary.insert(
        Name::from("BaseFont"),
        Object::Name(Name::from(font.name())),
    );
    if !matches!(font, StandardFont::Symbol | StandardFont::ZapfDingbats) {
        dictionary.insert(
            Name::from("Encoding"),
            Object::Name(Name::from("WinAnsiEncoding")),
        );
    }
    dictionary
}

// `content` with the name `from` in it, written after a slash, changed to `to`.
// Names in a canvas are only written by it, and its strings are hexadecimal, so
// nothing else can look like one.
fn replace_name(content: &[u8], from: &str, to: &str) -> Vec<u8> {
    let wanted = format!("/{from} ").into_bytes();
    let mut result = Vec::with_capacity(content.len());
    let mut at = 0;
    while at < content.len() {
        if content[at..].starts_with(&wanted) {
            result.extend_from_slice(format!("/{to} ").as_bytes());
            at += wanted.len();
        } else {
            result.push(content[at]);
            at += 1;
        }
    }
    result
}
