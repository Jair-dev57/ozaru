use super::{Catalog, Info, Page, read_catalog, read_info, read_pages};
use crate::base::{Diagnostics, Error, ErrorKind, Limits, Result};
use crate::xref::Resolver;

/// How to open a [`Document`].
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenOptions<'p> {
    pub limits: Limits,
    /// The user password of an encrypted file. With `None`, an encrypted file
    /// that needs a password still opens, locked: its pages can be counted, but
    /// its strings and streams can't be read until [`Document::unlock`].
    pub password: Option<&'p [u8]>,
}

/// A PDF document, opened: its pages and document-wide data, read once.
///
/// It is the entry point to the engine. It loads the cross-reference data,
/// repairing it if needed, then reads the catalog, the page tree and the
/// information dictionary, working around the damage it can. What it worked
/// around is in [`diagnostics`](Self::diagnostics).
#[derive(Debug)]
pub struct Document<'a> {
    resolver: Resolver<'a>,
    limits: Limits,
    catalog: Catalog,
    pages: Vec<Page>,
    info: Option<Info>,
    diagnostics: Diagnostics,
}

impl<'a> Document<'a> {
    /// Opens `file` with the default limits and no password.
    pub fn open(file: &'a [u8]) -> Result<Self> {
        Self::open_with(file, OpenOptions::default())
    }

    /// Opens `file` with `options`.
    ///
    /// It is an error if the cross-reference data can't be loaded or rebuilt, if
    /// there's no catalog or page tree, or if the page tree exceeds the limits.
    /// With a password, it is a `PasswordRequired` error if the file is encrypted
    /// and the password is wrong, or its encryption isn't supported. The password
    /// is ignored if the file isn't encrypted.
    pub fn open_with(file: &'a [u8], options: OpenOptions<'_>) -> Result<Self> {
        let mut resolver = Resolver::open(file, options.limits)?;
        if let Some(password) = options.password
            && resolver.is_encrypted()
            && !resolver.unlock(password)
        {
            let offset = resolver
                .trailer()
                .get("Encrypt")
                .and_then(|entry| entry.as_reference())
                .map_or(0, |id| resolver.object_offset(id.number));
            return Err(Error::new(ErrorKind::PasswordRequired, offset));
        }
        let structure = Structure::read(&mut resolver, options.limits)?;
        Ok(Self {
            resolver,
            limits: options.limits,
            catalog: structure.catalog,
            pages: structure.pages,
            info: structure.info,
            diagnostics: structure.diagnostics,
        })
    }

    /// Tries `password` as the user password, and returns whether it was right.
    /// The document is read again, decrypted, so its strings and streams can be
    /// read. It is `true` for a file that isn't encrypted or is unlocked already.
    pub fn unlock(&mut self, password: &[u8]) -> bool {
        if !self.resolver.needs_password() {
            return true;
        }
        if !self.resolver.unlock(password) {
            return false;
        }
        // The same objects are read, decrypted now, so it can't fail where
        // opening didn't. If it does anyway, the locked reading is kept.
        if let Ok(structure) = Structure::read(&mut self.resolver, self.limits) {
            self.catalog = structure.catalog;
            self.pages = structure.pages;
            self.info = structure.info;
            self.diagnostics = structure.diagnostics;
        }
        true
    }

    /// The number of pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The page at `index`, from 0. `None` past the last page.
    pub fn page(&self, index: usize) -> Option<&Page> {
        self.pages.get(index)
    }

    /// Every page, in order.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The information dictionary: title, author, dates. `None` if the file has
    /// none, or is still locked.
    pub fn info(&self) -> Option<&Info> {
        self.info.as_ref()
    }

    /// Whether the file is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.resolver.is_encrypted()
    }

    /// Whether the file is encrypted and can't be decrypted yet, until
    /// [`unlock`](Self::unlock) with its user password.
    pub fn needs_password(&self) -> bool {
        self.resolver.needs_password()
    }

    /// The resolver of the document's objects, to read what the document doesn't
    /// read itself, such as content streams.
    pub fn resolver(&mut self) -> &mut Resolver<'a> {
        &mut self.resolver
    }

    /// Every problem worked around so far: first those of loading and resolving
    /// objects, then those of reading the document's structure.
    pub fn diagnostics(&self) -> Diagnostics {
        let mut all = Diagnostics::new(self.limits.max_diagnostics);
        all.append(self.resolver.diagnostics());
        all.append(&self.diagnostics);
        all
    }
}

// What a document reads from its objects when it opens.
struct Structure {
    catalog: Catalog,
    pages: Vec<Page>,
    info: Option<Info>,
    diagnostics: Diagnostics,
}

impl Structure {
    fn read(resolver: &mut Resolver<'_>, limits: Limits) -> Result<Self> {
        let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
        let catalog = read_catalog(resolver, &mut diagnostics)?;
        let pages = read_pages(resolver, &catalog, limits, &mut diagnostics)?;
        let info = read_info(resolver, &mut diagnostics);
        Ok(Self {
            catalog,
            pages,
            info,
            diagnostics,
        })
    }
}
