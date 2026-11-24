use pyo3::prelude::*;

pyo3::create_exception!(
    ozaru,
    PasswordError,
    pyo3::exceptions::PyValueError,
    "Raised when a PDF needs a password, or the password given is incorrect."
);

pyo3::create_exception!(
    ozaru,
    PageWarning,
    pyo3::exceptions::PyUserWarning,
    "Warns that the text or the images of one page could not be extracted."
);

#[pymodule]
mod ozaru {
    #[pymodule_export]
    use super::PageWarning;
    #[pymodule_export]
    use super::PasswordError;

    use ozaru_engine::base::ErrorKind as EngineError;
    use ozaru_engine::base::{Diagnostics, Limits};
    use ozaru_engine::document::{
        Annotation as EngineAnnotation, Date, FieldKind, FieldValue, FormField as EngineField,
        Info, Link, Page, Rectangle, Xmp, read_catalog, read_xmp,
    };
    use ozaru_engine::images::{ImageFormat, document_images};
    use ozaru_engine::render::{RenderOptions, render_page as draw_page};
    use ozaru_engine::text::{PageProblem, TextBlock, document_blocks, document_text, line_text};
    use ozaru_engine::writer::{Editor, Encryption, RANDOM_BYTES, fill_field};
    use ozaru_engine::xref::Resolver;
    use ozaru_engine::{Document, OpenOptions};
    use pyo3::exceptions::{
        PyFileNotFoundError, PyIndexError, PyIsADirectoryError, PyNotADirectoryError, PyOSError,
        PyPermissionError, PyValueError,
    };
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Once};
    use std::{fs, io, panic, thread};

    const STACK_SIZE: usize = 8 * 1024 * 1024;

    // The name of the threads that read PDFs, whose panics are handled.
    const WORKER: &str = "ozaru-reader";

    // errno values shared by Linux, macOS and Windows, as in Python's errno module.
    const ENOENT: i32 = 2;
    const EACCES: i32 = 13;
    const ENOTDIR: i32 = 20;
    const EISDIR: i32 = 21;

    // Maps a file error to the exception Python's open() would raise, with errno and filename.
    fn file_error(error: io::Error, path: &Path) -> PyErr {
        io_error(error, path, "read")
    }

    // The same for a file that `action`, such as "write", failed on.
    fn io_error(error: io::Error, path: &Path, action: &str) -> PyErr {
        let filename = path.to_string_lossy().into_owned();
        match error.kind() {
            ErrorKind::NotFound => {
                PyFileNotFoundError::new_err((ENOENT, "No such file or directory", filename))
            }
            ErrorKind::PermissionDenied => {
                PyPermissionError::new_err((EACCES, "Permission denied", filename))
            }
            ErrorKind::IsADirectory => is_a_directory(path),
            ErrorKind::NotADirectory => {
                PyNotADirectoryError::new_err((ENOTDIR, "Not a directory", filename))
            }
            ErrorKind::InvalidInput if path.as_os_str().as_encoded_bytes().contains(&0) => {
                PyValueError::new_err("The path contains a null byte")
            }
            _ => PyOSError::new_err(format!("Could not {action} {}: {error}", path.display())),
        }
    }

    fn is_a_directory(path: &Path) -> PyErr {
        PyIsADirectoryError::new_err((
            EISDIR,
            "Is a directory",
            path.to_string_lossy().into_owned(),
        ))
    }

    // Reads the whole file, so file errors are ours to report.
    fn read_file(path: &Path) -> PyResult<Arc<Vec<u8>>> {
        // Windows reports a directory as "access denied", so check for it first.
        if fs::metadata(path)
            .map_err(|e| file_error(e, path))?
            .is_dir()
        {
            return Err(is_a_directory(path));
        }
        fs::read(path)
            .map(Arc::new)
            .map_err(|e| file_error(e, path))
    }


    // Where a PDF comes from: a file on disk, or bytes already in memory (given
    // as bytes, as a file-like object, or fetched from a URL).
    enum Source {
        Path(PathBuf),
        Data(Arc<Vec<u8>>),
    }

    // The most a URL may give: a limit against a server that never stops.
    const URL_LIMIT: usize = 1 << 30;

    const FETCH: &std::ffi::CStr = cr#"
import urllib.request

def fetch(url, limit):
    request = urllib.request.Request(url, headers={"User-Agent": "ozaru"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("The PDF at the URL is too large")
    return data
"#;

    impl Source {
        // The bytes of the PDF. A file is read here, so callers can do it
        // without holding the interpreter.
        fn load(&self) -> PyResult<Arc<Vec<u8>>> {
            match self {
                Source::Path(path) => read_file(path),
                Source::Data(data) => Ok(Arc::clone(data)),
            }
        }

        // The name of the file, if it has one.
        fn name(&self) -> Option<&Path> {
            match self {
                Source::Path(path) => Some(path),
                Source::Data(_) => None,
            }
        }

        // What to call it in a message.
        fn label(&self) -> String {
            match self {
                Source::Path(path) => path.display().to_string(),
                Source::Data(_) => "the PDF from memory".to_string(),
            }
        }
    }

    impl<'a, 'py> FromPyObject<'a, 'py> for Source {
        type Error = PyErr;

        fn extract(object: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
            let py = object.py();
            if let Ok(bytes) = object.cast::<PyBytes>() {
                return Ok(Source::Data(Arc::new(bytes.as_bytes().to_vec())));
            }
            if let Ok(text) = object.cast::<PyString>() {
                let text = text.to_str()?;
                if text.starts_with("http://") || text.starts_with("https://") {
                    let module = PyModule::from_code(py, FETCH, c"fetch.py", c"ozaru_fetch")?;
                    let data = module.call_method1("fetch", (text, URL_LIMIT))?;
                    let data = data.cast::<PyBytes>()?.as_bytes().to_vec();
                    return Ok(Source::Data(Arc::new(data)));
                }
                return Ok(Source::Path(PathBuf::from(text)));
            }
            // A bytearray, a memoryview or any other buffer.
            if object.hasattr("__buffer__")? || object.is_instance_of::<pyo3::types::PyByteArray>() {
                let bytes = py.import("builtins")?.getattr("bytes")?.call1((object,))?;
                let data = bytes.cast::<PyBytes>()?.as_bytes().to_vec();
                return Ok(Source::Data(Arc::new(data)));
            }
            // A file-like object, opened in binary mode: not a path.
            if object.hasattr("read")? && !object.hasattr("__fspath__")? {
                let read = object.call_method0("read")?;
                return match read.cast::<PyBytes>() {
                    Ok(bytes) => Ok(Source::Data(Arc::new(bytes.as_bytes().to_vec()))),
                    Err(_) => Err(PyValueError::new_err(
                        "The file must be opened in binary mode: use open(path, \"rb\")",
                    )),
                };
            }
            object
                .extract::<PathBuf>()
                .map(Source::Path)
                .map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "Expected a path, bytes, a file opened in binary mode or a URL",
                    )
                })
        }
    }

    // Writes the whole file, replacing it if it exists.
    fn write_file(path: &Path, data: &[u8]) -> PyResult<()> {
        if path.is_dir() {
            return Err(is_a_directory(path));
        }
        fs::write(path, data).map_err(|e| io_error(e, path, "write"))
    }

    const PROTECTED: &str =
        "The PDF is protected with a password: pass it with the password argument";
    const WRONG_PASSWORD: &str = "The password is incorrect";

    fn password_error(message: &str) -> PyErr {
        PasswordError::new_err(message.to_string())
    }

    // SASLprep (RFC 4013), which revisions 5 and 6 of PDF encryption apply to
    // passwords before encoding them in UTF-8: special spaces become spaces,
    // invisible characters go, and NFKC normalization turns, for example, the
    // Roman numeral Ⅸ into IX. `None` if the password has a character the profile
    // prohibits. It uses Python's own Unicode tables.
    const SASLPREP: &std::ffi::CStr = cr#"
import stringprep
import unicodedata

PROHIBITED = (
    stringprep.in_table_c12, stringprep.in_table_c21_c22, stringprep.in_table_c3,
    stringprep.in_table_c4, stringprep.in_table_c5, stringprep.in_table_c6,
    stringprep.in_table_c7, stringprep.in_table_c8, stringprep.in_table_c9,
)

def saslprep(text):
    text = "".join(
        " " if stringprep.in_table_c12(c) else c
        for c in text
        if not stringprep.in_table_b1(c)
    )
    text = unicodedata.normalize("NFKC", text)
    if any(test(c) for c in text for test in PROHIBITED):
        return None
    if any(stringprep.in_table_d1(c) for c in text):
        if any(stringprep.in_table_d2(c) for c in text):
            return None
        if not (stringprep.in_table_d1(text[0]) and stringprep.in_table_d1(text[-1])):
            return None
    return text
"#;

    // The bytes to try for `password`, most likely first: its UTF-8 as given,
    // after SASLprep for revisions 5 and 6, and in Latin-1, close to the
    // PDFDocEncoding of revisions 2 to 4.
    fn password_candidates(
        py: Python<'_>,
        password: Option<String>,
    ) -> PyResult<Option<Vec<Vec<u8>>>> {
        let Some(password) = password else {
            return Ok(None);
        };
        let mut candidates = vec![password.as_bytes().to_vec()];
        let module = PyModule::from_code(py, SASLPREP, c"saslprep.py", c"ozaru_saslprep")?;
        let prepared: Option<String> = module.call_method1("saslprep", (&password,))?.extract()?;
        if let Some(prepared) = prepared {
            candidates.push(prepared.into_bytes());
        }
        let latin_1: Option<Vec<u8>> = password
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect();
        candidates.extend(latin_1);
        let mut unique: Vec<Vec<u8>> = Vec::new();
        for candidate in candidates {
            if !unique.contains(&candidate) {
                unique.push(candidate);
            }
        }
        Ok(Some(unique))
    }

    // Opens the file with the engine. Without a password, encrypted files are
    // opened locked, so their pages can usually be counted anyway. `protected` is
    // the message for files whose page tree is encrypted too.
    fn open_document<'a>(
        data: &'a [u8],
        password: Option<&[Vec<u8>]>,
        protected: &str,
    ) -> PyResult<Document<'a>> {
        let error = match password {
            None => match Document::open(data) {
                Ok(document) => return Ok(document),
                Err(error) => error,
            },
            Some(password) => {
                let mut last = None;
                for candidate in password {
                    let options = OpenOptions {
                        password: Some(candidate),
                        ..OpenOptions::default()
                    };
                    match Document::open_with(data, options) {
                        Ok(document) => return Ok(document),
                        Err(error) => last = Some(error),
                    }
                }
                let error = last.expect("there is always a candidate");
                if error.kind() == EngineError::PasswordRequired {
                    return Err(password_error(WRONG_PASSWORD));
                }
                error
            }
        };
        let message = match error.kind() {
            EngineError::PasswordRequired => return Err(password_error(protected)),
            // The file has PDF objects, but not a usable page tree.
            EngineError::MissingCatalog
            | EngineError::InvalidPageTree
            | EngineError::LimitExceeded(_) => {
                format!("Could not read the PDF: the file may be damaged ({error})")
            }
            _ => format!("Could not open the PDF: {error}"),
        };
        Err(PyValueError::new_err(message))
    }

    fn count_pages(data: &[u8], password: Option<&[Vec<u8>]>) -> PyResult<usize> {
        let message = "Could not count the pages: the PDF is protected with a password: \
                       pass it with the password argument";
        Ok(open_document(data, password, message)?.page_count())
    }

    // The information dictionary, read with the engine. Only the objects are
    // loaded, not the page tree, so a file whose pages are damaged still gives
    // its information. A file that needs a password is an error: its information
    // is encrypted.
    fn read_info(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Option<Info>, Option<Xmp>)> {
        let limits = Limits::default();
        let mut resolver = Resolver::open(data, limits)
            .map_err(|e| PyValueError::new_err(format!("Could not open the PDF: {e}")))?;
        if resolver.needs_password() {
            let Some(password) = password else {
                return Err(password_error(PROTECTED));
            };
            let unlocked = password.iter().any(|candidate| resolver.unlock(candidate));
            if !unlocked {
                return Err(password_error(WRONG_PASSWORD));
            }
        }
        let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
        let info = ozaru_engine::document::read_info(&mut resolver, &mut diagnostics);
        // The XMP packet is in the catalog: a file whose catalog is damaged
        // still gives its information dictionary.
        let xmp = read_catalog(&mut resolver, &mut diagnostics)
            .ok()
            .and_then(|catalog| read_xmp(&mut resolver, catalog.dictionary()));
        Ok((info, xmp))
    }

    /// The document information of a PDF: its title, author and dates.
    ///
    /// Each attribute is `None` when the PDF doesn't have it.
    #[pyclass(frozen, module = "ozaru")]
    struct Metadata {
        #[pyo3(get)]
        title: Option<String>,
        #[pyo3(get)]
        author: Option<String>,
        #[pyo3(get)]
        subject: Option<String>,
        #[pyo3(get)]
        keywords: Option<String>,
        #[pyo3(get)]
        creator: Option<String>,
        #[pyo3(get)]
        producer: Option<String>,
        #[pyo3(get)]
        creation_date: Option<Py<PyAny>>,
        #[pyo3(get)]
        modification_date: Option<Py<PyAny>>,
        #[pyo3(get)]
        xmp: Option<String>,
    }

    #[pymethods]
    impl Metadata {
        // Equal when every attribute is, and hashable, so it can be a dictionary
        // key or go in a set.
        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let repr = |value: Option<Bound<'_, PyAny>>| -> PyResult<String> {
                match value {
                    Some(value) => Ok(value.repr()?.to_string()),
                    None => Ok("None".to_string()),
                }
            };
            let text = |value: &Option<String>| {
                repr(value.as_ref().map(|v| PyString::new(py, v).into_any()))
            };
            let date = |value: &Option<Py<PyAny>>| repr(value.as_ref().map(|v| v.bind(py).clone()));
            Ok(format!(
                "Metadata(title={}, author={}, subject={}, keywords={}, creator={}, \
                 producer={}, creation_date={}, modification_date={})",
                text(&self.title)?,
                text(&self.author)?,
                text(&self.subject)?,
                text(&self.keywords)?,
                text(&self.creator)?,
                text(&self.producer)?,
                date(&self.creation_date)?,
                date(&self.modification_date)?,
            ))
        }
    }

    /// The size and rotation of one page of a PDF.
    ///
    /// Sizes are in points, 1/72 of an inch, as in every PDF.
    #[pyclass(frozen, module = "ozaru")]
    struct PageInfo {
        #[pyo3(get)]
        number: usize,
        #[pyo3(get)]
        width: f64,
        #[pyo3(get)]
        height: f64,
        #[pyo3(get)]
        rotation: u16,
        #[pyo3(get)]
        media_box: (f64, f64, f64, f64),
        #[pyo3(get)]
        crop_box: (f64, f64, f64, f64),
    }

    #[pymethods]
    impl PageInfo {
        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self) -> String {
            format!(
                "PageInfo(number={}, width={:?}, height={:?}, rotation={})",
                self.number, self.width, self.height, self.rotation
            )
        }
    }

    /// A word of a PDF, and where it is on its page.
    ///
    /// The box is in points, 1/72 of an inch, from the top left corner of the
    /// visible area of the page, before the page is rotated: `x0` and `x1` go
    /// right, and `y0` and `y1` go down.
    #[pyclass(frozen, module = "ozaru")]
    struct Word {
        #[pyo3(get)]
        page: usize,
        #[pyo3(get)]
        block: usize,
        #[pyo3(get)]
        line: usize,
        #[pyo3(get)]
        text: String,
        #[pyo3(get)]
        x0: f64,
        #[pyo3(get)]
        y0: f64,
        #[pyo3(get)]
        x1: f64,
        #[pyo3(get)]
        y1: f64,
    }

    #[pymethods]
    impl Word {
        /// The box as a tuple: `(x0, y0, x1, y1)`.
        #[getter]
        fn bbox(&self) -> (f64, f64, f64, f64) {
            (self.x0, self.y0, self.x1, self.y1)
        }

        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let text = PyString::new(py, &self.text).repr()?;
            Ok(format!(
                "Word(page={}, block={}, line={}, text={text}, x0={:.2}, y0={:.2}, x1={:.2}, y1={:.2})",
                self.page, self.block, self.line, self.x0, self.y0, self.x1, self.y1
            ))
        }
    }

    impl Word {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            (
                self.page,
                self.block,
                self.line,
                self.text.clone(),
                self.x0,
                self.y0,
                self.x1,
                self.y1,
            )
                .into_pyobject(py)
        }
    }

    /// A block of text of a PDF: a paragraph, a heading or a cell, and where it
    /// is on its page.
    ///
    /// The box is measured as the box of a `Word` is.
    #[pyclass(frozen, module = "ozaru")]
    struct Block {
        #[pyo3(get)]
        page: usize,
        #[pyo3(get)]
        number: usize,
        #[pyo3(get)]
        text: String,
        #[pyo3(get)]
        x0: f64,
        #[pyo3(get)]
        y0: f64,
        #[pyo3(get)]
        x1: f64,
        #[pyo3(get)]
        y1: f64,
    }

    #[pymethods]
    impl Block {
        /// The box as a tuple: `(x0, y0, x1, y1)`.
        #[getter]
        fn bbox(&self) -> (f64, f64, f64, f64) {
            (self.x0, self.y0, self.x1, self.y1)
        }

        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let text = PyString::new(py, &self.text).repr()?;
            Ok(format!(
                "Block(page={}, number={}, text={text}, x0={:.2}, y0={:.2}, x1={:.2}, y1={:.2})",
                self.page, self.number, self.x0, self.y0, self.x1, self.y1
            ))
        }
    }

    impl Block {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            (
                self.page,
                self.number,
                self.text.clone(),
                self.x0,
                self.y0,
                self.x1,
                self.y1,
            )
                .into_pyobject(py)
        }
    }

    /// An image of a PDF, and where it is on its page.
    ///
    /// The box is measured as the box of a `Word` is.
    #[pyclass(frozen, module = "ozaru")]
    struct Image {
        #[pyo3(get)]
        page: usize,
        #[pyo3(get)]
        number: usize,
        #[pyo3(get)]
        x0: f64,
        #[pyo3(get)]
        y0: f64,
        #[pyo3(get)]
        x1: f64,
        #[pyo3(get)]
        y1: f64,
        #[pyo3(get)]
        width: Option<u32>,
        #[pyo3(get)]
        height: Option<u32>,
        #[pyo3(get)]
        format: Option<&'static str>,
        file: Option<Vec<u8>>,
        // Why there is no file, if there isn't.
        missing: &'static str,
    }

    #[pymethods]
    impl Image {
        /// The box as a tuple: `(x0, y0, x1, y1)`.
        #[getter]
        fn bbox(&self) -> (f64, f64, f64, f64) {
            (self.x0, self.y0, self.x1, self.y1)
        }

        /// The image file, in its `format`: `None` if ozaru can't give it yet.
        #[getter]
        fn data<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
            self.file.as_deref().map(|data| PyBytes::new(py, data))
        }

        /// Writes the image file to `path`.
        fn save(&self, py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<()> {
            let Some(data) = &self.file else {
                return Err(PyValueError::new_err(format!(
                    "Could not save the image: {}",
                    self.missing
                )));
            };
            let file = py.import("builtins")?.call_method1("open", (path, "wb"))?;
            let written = file.call_method1("write", (PyBytes::new(py, data),));
            file.call_method0("close")?;
            written.map(|_| ())
        }

        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self) -> String {
            let size = match (self.width, self.height) {
                (Some(width), Some(height)) => format!("{width}x{height}"),
                _ => "unknown".to_string(),
            };
            let format = self.format.map_or("None".to_string(), |f| format!("'{f}'"));
            format!(
                "Image(page={}, number={}, size={size}, format={format}, x0={:.2}, y0={:.2}, x1={:.2}, y1={:.2})",
                self.page, self.number, self.x0, self.y0, self.x1, self.y1
            )
        }
    }

    impl Image {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            (
                (self.page, self.number, self.x0, self.y0, self.x1, self.y1),
                (self.width, self.height, self.format),
                self.file.as_deref().map(|data| PyBytes::new(py, data)),
            )
                .into_pyobject(py)
        }
    }

    impl Metadata {
        // Every attribute, as a Python tuple, to compare and hash.
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            let date = |value: &Option<Py<PyAny>>| value.as_ref().map(|v| v.clone_ref(py));
            (
                self.title.clone(),
                self.author.clone(),
                self.subject.clone(),
                self.keywords.clone(),
                self.creator.clone(),
                self.producer.clone(),
                date(&self.creation_date),
                date(&self.modification_date),
                self.xmp.clone(),
            )
                .into_pyobject(py)
        }
    }

    impl PageInfo {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            (
                self.number,
                self.width,
                self.height,
                self.rotation,
                self.media_box,
                self.crop_box,
            )
                .into_pyobject(py)
        }
    }

    fn corners(rectangle: Rectangle) -> (f64, f64, f64, f64) {
        (
            rectangle.left,
            rectangle.bottom,
            rectangle.right,
            rectangle.top,
        )
    }

    // The page as a viewer shows it: its visible area, turned by its rotation.
    fn page_info_of(number: usize, page: &Page) -> PageInfo {
        let visible = page.crop_box();
        let (width, height) = match page.rotation() {
            90 | 270 => (visible.height(), visible.width()),
            _ => (visible.width(), visible.height()),
        };
        PageInfo {
            number,
            width,
            height,
            rotation: page.rotation(),
            media_box: corners(page.media_box()),
            crop_box: corners(visible),
        }
    }

    // Every page, and the numbers of the damaged ones, whose object can't be
    // read: they get the size and rotation of their ancestors, or the defaults.
    fn read_page_info(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<PageInfo>, Vec<usize>)> {
        let message = "Could not read the pages: the PDF is protected with a password: \
                       pass it with the password argument";
        let document = open_document(data, password, message)?;
        let pages = document.pages().iter().enumerate();
        let info = pages
            .clone()
            .map(|(index, page)| page_info_of(index + 1, page))
            .collect();
        let damaged = pages
            .filter(|(_, page)| page.is_damaged())
            .map(|(index, _)| index + 1)
            .collect();
        Ok((info, damaged))
    }

    /// An annotation of a page: a link, a note, a highlight or the widget of a form
    /// field.
    ///
    /// The `rect` and `quad_points` are in points, from the top left corner of
    /// the visible area of the page, before the page is rotated, as for `Word`:
    /// `y` goes down. Attributes the annotation doesn't have are `None`.
    #[pyclass(frozen, module = "ozaru")]
    struct Annotation {
        #[pyo3(get)]
        page: usize,
        #[pyo3(get)]
        subtype: String,
        #[pyo3(get)]
        rect: (f64, f64, f64, f64),
        #[pyo3(get)]
        contents: Option<String>,
        #[pyo3(get)]
        name: Option<String>,
        #[pyo3(get)]
        author: Option<String>,
        #[pyo3(get)]
        subject: Option<String>,
        #[pyo3(get)]
        modified: Option<Py<PyAny>>,
        #[pyo3(get)]
        flags: u32,
        #[pyo3(get)]
        color: Option<Vec<f64>>,
        #[pyo3(get)]
        quad_points: Vec<Quad>,
        #[pyo3(get)]
        uri: Option<String>,
        #[pyo3(get)]
        target_page: Option<usize>,
        #[pyo3(get)]
        has_appearance: bool,
    }

    #[pymethods]
    impl Annotation {
        fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
            let py = other.py();
            match other.cast::<Self>() {
                Ok(other) => self.fields(py)?.eq(other.get().fields(py)?),
                Err(_) => Ok(false),
            }
        }

        fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
            self.fields(py)?.hash()
        }

        fn __repr__(&self) -> String {
            format!(
                "Annotation(page={}, subtype={:?}, rect=({:.2}, {:.2}, {:.2}, {:.2}))",
                self.page, self.subtype, self.rect.0, self.rect.1, self.rect.2, self.rect.3
            )
        }
    }

    impl Annotation {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            // Lists can't be hashed: tuples stand for them.
            let color = self
                .color
                .as_ref()
                .map(|c| PyTuple::new(py, c))
                .transpose()?;
            let quads = PyTuple::new(py, &self.quad_points)?;
            (
                self.page,
                &self.subtype,
                self.rect,
                &self.contents,
                &self.name,
                &self.author,
                &self.subject,
                self.flags,
                color,
                quads,
                &self.uri,
                self.target_page,
            )
                .into_pyobject(py)
        }
    }

    // The annotations of every page, each with its page number and visible
    // area, and the numbers of the damaged pages.
    type Quad = (f64, f64, f64, f64, f64, f64, f64, f64);

    type Found = Vec<(usize, Rectangle, EngineAnnotation)>;

    fn read_all_annotations(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Found, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        let damaged = damaged_pages(&document);
        let mut all = Vec::new();
        for index in 0..document.page_count() {
            let Some(crop) = document.page(index).map(Page::crop_box) else {
                continue;
            };
            for found in document.annotations(index) {
                all.push((index + 1, crop, found));
            }
        }
        Ok((all, damaged))
    }

    fn to_annotation(
        py: Python<'_>,
        page: usize,
        crop: Rectangle,
        found: EngineAnnotation,
    ) -> PyResult<Annotation> {
        let down = |x: f64, y: f64| (x - crop.left, crop.top - y);
        let (x0, y0) = down(found.rect.left, found.rect.top);
        let (x1, y1) = down(found.rect.right, found.rect.bottom);
        let (uri, target_page) = match &found.link {
            Some(Link::Uri(uri)) => (Some(uri.clone()), None),
            Some(Link::Page(page)) => (None, Some(page + 1)),
            _ => (None, None),
        };
        let modified = found
            .modified
            .map(|date| to_datetime(py, date))
            .transpose()?;
        let quad_points = found
            .quad_points
            .iter()
            .map(|q| {
                let (a, b) = down(q[0], q[1]);
                let (c, d) = down(q[2], q[3]);
                let (e, f) = down(q[4], q[5]);
                let (g, h) = down(q[6], q[7]);
                (a, b, c, d, e, f, g, h)
            })
            .collect();
        Ok(Annotation {
            page,
            subtype: found.subtype,
            rect: (x0, y0, x1, y1),
            contents: found.contents,
            name: found.name,
            author: found.author,
            subject: found.subject,
            modified,
            flags: found.flags,
            color: (!found.color.is_empty()).then_some(found.color),
            quad_points,
            uri,
            target_page,
            has_appearance: found.appearance.is_some(),
        })
    }

    /// Return the annotations of every page of the PDF: links, notes, highlights
    /// and the widgets of form fields.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn annotations(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<Annotation>> {
        let password = password_candidates(py, password)?;
        let (found, damaged_pages) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || read_all_annotations(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        let reason = "the page may be damaged, so its annotations may be missing";
        for page in damaged_pages {
            warn_page(
                py,
                page,
                format!("Could not read page {page}: {reason}"),
                reason,
            )?;
        }
        found
            .into_iter()
            .map(|(page, crop, found)| to_annotation(py, page, crop, found))
            .collect()
    }

    /// A field of the form of a PDF: a text box, a check box, a drop-down list...
    ///
    /// `value` is a `str` for text and choices, a `bool` for check boxes, the
    /// name of the chosen button for radio buttons, a list of `str` for lists that
    /// allow several, and `None` when the field has no value. `rect` is where
    /// its first widget is, in the coordinates of `Annotation.rect`.
    #[pyclass(frozen, module = "ozaru")]
    struct FormField {
        #[pyo3(get)]
        name: String,
        #[pyo3(get)]
        kind: &'static str,
        #[pyo3(get)]
        value: Option<Py<PyAny>>,
        #[pyo3(get)]
        default_value: Option<Py<PyAny>>,
        #[pyo3(get)]
        read_only: bool,
        #[pyo3(get)]
        required: bool,
        #[pyo3(get)]
        multiline: bool,
        #[pyo3(get)]
        max_length: Option<usize>,
        #[pyo3(get)]
        options: Vec<(String, String)>,
        #[pyo3(get)]
        page: Option<usize>,
        #[pyo3(get)]
        rect: Option<(f64, f64, f64, f64)>,
    }

    #[pymethods]
    impl FormField {
        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let value = match &self.value {
                Some(value) => value.bind(py).repr()?.to_string(),
                None => "None".to_string(),
            };
            Ok(format!(
                "FormField(name={:?}, kind={:?}, value={value})",
                self.name, self.kind
            ))
        }
    }

    fn kind_name(kind: FieldKind) -> &'static str {
        match kind {
            FieldKind::Text => "text",
            FieldKind::Checkbox => "checkbox",
            FieldKind::Radio => "radio",
            FieldKind::PushButton => "button",
            FieldKind::Combo => "combo",
            FieldKind::List => "list",
            FieldKind::Signature => "signature",
            FieldKind::Unknown => "unknown",
        }
    }

    type FoundFields = Vec<(EngineField, Option<Rectangle>)>;

    // The fields of the form, with the visible area of each page.
    fn read_all_fields(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(FoundFields, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        let damaged = damaged_pages(&document);
        let fields = document.form_fields();
        let boxes: Vec<Rectangle> = document.pages().iter().map(Page::crop_box).collect();
        let found = fields
            .into_iter()
            .map(|field| {
                let crop = field
                    .widgets
                    .first()
                    .and_then(|widget| widget.page)
                    .and_then(|page| boxes.get(page).copied());
                (field, crop)
            })
            .collect();
        Ok((found, damaged))
    }

    // A value as Python shows it: a bool for check boxes, the chosen button
    // for radio buttons (`None` when off), and the text or list otherwise.
    fn value_to_python(
        py: Python<'_>,
        kind: FieldKind,
        value: &Option<FieldValue>,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(value) = value else { return Ok(None) };
        Ok(match (kind, value) {
            (FieldKind::Checkbox, FieldValue::Text(state)) => Some(
                PyBool::new(py, state != "Off")
                    .to_owned()
                    .into_any()
                    .unbind(),
            ),
            (FieldKind::Radio, FieldValue::Text(state)) if state == "Off" => None,
            (_, FieldValue::Text(text)) => Some(PyString::new(py, text).into_any().unbind()),
            (_, FieldValue::Choices(choices)) => {
                Some(PyList::new(py, choices)?.into_any().unbind())
            }
        })
    }

    fn to_form_field(
        py: Python<'_>,
        field: EngineField,
        crop: Option<Rectangle>,
    ) -> PyResult<FormField> {
        let first = field.widgets.first();
        let rect = first.zip(crop).map(|(widget, crop)| {
            (
                widget.rect.left - crop.left,
                crop.top - widget.rect.top,
                widget.rect.right - crop.left,
                crop.top - widget.rect.bottom,
            )
        });
        Ok(FormField {
            kind: kind_name(field.kind),
            value: value_to_python(py, field.kind, &field.value)?,
            default_value: value_to_python(py, field.kind, &field.default)?,
            read_only: field.is_read_only(),
            required: field.is_required(),
            multiline: field.is_multiline(),
            max_length: field.max_length,
            page: first.and_then(|widget| widget.page).map(|page| page + 1),
            rect,
            options: field.options,
            name: field.name,
        })
    }

    /// Return the fields of the form of the PDF, with their values. Empty if the
    /// PDF has no form.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn form_fields(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<FormField>> {
        let password = password_candidates(py, password)?;
        let (found, damaged_pages) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || read_all_fields(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        let reason = "the page may be damaged, so its form fields may be missing";
        for page in damaged_pages {
            warn_page(
                py,
                page,
                format!("Could not read page {page}: {reason}"),
                reason,
            )?;
        }
        found
            .into_iter()
            .map(|(field, crop)| to_form_field(py, field, crop))
            .collect()
    }

    // What the user asked a field to hold.
    enum Wanted {
        Clear,
        Flag(bool),
        Text(String),
        Choices(Vec<String>),
    }

    fn wanted_from(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Wanted> {
        if value.is_none() {
            return Ok(Wanted::Clear);
        }
        // A bool is also an int, so it is asked first.
        if let Ok(flag) = value.cast::<PyBool>() {
            return Ok(Wanted::Flag(flag.is_true()));
        }
        if let Ok(text) = value.extract::<String>() {
            return Ok(Wanted::Text(text));
        }
        if let Ok(choices) = value.extract::<Vec<String>>() {
            return Ok(Wanted::Choices(choices));
        }
        Err(PyValueError::new_err(format!(
            "The value for the field {name:?} must be a str, a bool, a list of str or None"
        )))
    }

    // The engine's value that `wanted` means for `field`, or why not.
    fn value_for(field: &EngineField, wanted: Wanted) -> Option<FieldValue> {
        let text = |text: String| Some(FieldValue::Text(text));
        match (field.kind, wanted) {
            (FieldKind::Checkbox, Wanted::Flag(on)) => {
                let on_state = field
                    .widgets
                    .iter()
                    .find_map(|widget| widget.on_state.clone())
                    .unwrap_or_else(|| "Yes".to_string());
                text(if on { on_state } else { "Off".to_string() })
            }
            (FieldKind::Checkbox | FieldKind::Radio, Wanted::Clear) => text("Off".to_string()),
            (FieldKind::Checkbox | FieldKind::Radio, Wanted::Text(state)) => text(state),
            (FieldKind::Text | FieldKind::Combo, Wanted::Clear) => text(String::new()),
            (FieldKind::Text | FieldKind::Combo, Wanted::Text(value)) => text(value),
            (FieldKind::List, Wanted::Clear) => Some(FieldValue::Choices(Vec::new())),
            (FieldKind::List, Wanted::Text(value)) => Some(FieldValue::Choices(vec![value])),
            (FieldKind::List, Wanted::Choices(values)) => Some(FieldValue::Choices(values)),
            _ => None,
        }
    }

    fn fill_and_save(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
        values: Vec<(String, Wanted)>,
        incremental: bool,
    ) -> PyResult<(Vec<u8>, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        let damaged = damaged_pages(&document);
        let fields = document.form_fields();
        let mut editor = Editor::new(&mut document).map_err(save_error)?;
        for (name, wanted) in values {
            let Some(field) = fields.iter().find(|field| field.name == name) else {
                let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                return Err(PyValueError::new_err(format!(
                    "The PDF has no field named {name:?}. Its fields are: {}",
                    names.join(", ")
                )));
            };
            let cannot = || {
                PyValueError::new_err(format!(
                    "The field {name:?} is a {} and can't hold that value",
                    kind_name(field.kind)
                ))
            };
            let value = value_for(field, wanted).ok_or_else(cannot)?;
            fill_field(&mut editor, field, &value).map_err(|_| cannot())?;
        }
        let saved = if incremental {
            editor.save_incremental()
        } else {
            editor.save()
        };
        Ok((saved.map_err(save_error)?, damaged))
    }

    /// Fill fields of the form of the PDF and save the result to another file.
    ///
    /// `values` maps the name of each field to its value: a `str` for text and
    /// choices, a `bool` for check boxes, the name of a button for radio buttons,
    /// a list of `str` for lists that allow several, and `None` to clear it.
    #[pyfunction]
    #[pyo3(signature = (path, output, values, *, password = None, incremental = false))]
    fn fill_form(
        py: Python<'_>,
        path: Source,
        output: PathBuf,
        values: &Bound<'_, PyDict>,
        password: Option<String>,
        incremental: bool,
    ) -> PyResult<()> {
        let mut wanted = Vec::new();
        for (name, value) in values.iter() {
            let name: String = name
                .extract()
                .map_err(|_| PyValueError::new_err("The names of the fields must be strings"))?;
            let value = wanted_from(&name, &value)?;
            wanted.push((name, value));
        }
        let password = password_candidates(py, password)?;
        let damaged = py.detach(|| {
            let data = path.load()?;
            let (saved, damaged) = in_new_thread(move || {
                fill_and_save(&data, password.as_deref(), wanted, incremental)
            })
            .unwrap_or_else(|_| Err(damaged()))?;
            write_file(&output, &saved)?;
            Ok::<_, PyErr>(damaged)
        })?;
        warn_damaged(py, &damaged)
    }

    // The permissions of the standard's table 22, by the name this module gives.
    const PERMISSIONS: [(&str, u32); 8] = [
        ("print", 1 << 2),
        ("modify", 1 << 3),
        ("copy", 1 << 4),
        ("annotate", 1 << 5),
        ("fill_forms", 1 << 8),
        ("accessibility", 1 << 9),
        ("assemble", 1 << 10),
        ("print_high_quality", 1 << 11),
    ];

    // The bits of the permissions named, or an error naming the one that isn't.
    fn permission_bits(names: Option<Vec<String>>) -> PyResult<u32> {
        let Some(names) = names else {
            return Ok(PERMISSIONS.iter().fold(0, |bits, (_, bit)| bits | bit));
        };
        let mut bits = 0;
        for name in names {
            let Some((_, bit)) = PERMISSIONS.iter().find(|(known, _)| *known == name) else {
                let known: Vec<&str> = PERMISSIONS.iter().map(|(name, _)| *name).collect();
                return Err(PyValueError::new_err(format!(
                    "{name:?} isn't a permission. They are: {}",
                    known.join(", ")
                )));
            };
            bits |= bit;
        }
        Ok(bits)
    }

    // A password to protect a file with, as PDF 2.0 stores it: UTF-8 after SASLprep.
    fn new_password(py: Python<'_>, password: &str) -> PyResult<Vec<u8>> {
        let module = PyModule::from_code(py, SASLPREP, c"saslprep.py", c"ozaru_saslprep")?;
        let prepared: Option<String> = module.call_method1("saslprep", (password,))?.extract()?;
        prepared.map(String::into_bytes).ok_or_else(|| {
            PyValueError::new_err("The password has characters that PDF passwords can't have")
        })
    }

    fn encrypt_and_save(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
        encryption: &Encryption<'_>,
        random: &[u8],
    ) -> PyResult<(Vec<u8>, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        let damaged = damaged_pages(&document);
        let mut editor = Editor::new(&mut document).map_err(save_error)?;
        let saved = editor
            .save_encrypted(encryption, random)
            .map_err(save_error)?;
        Ok((saved, damaged))
    }

    /// Protect the PDF with passwords and save the result to another file, with
    /// AES-256.
    ///
    /// Anyone who knows `user_password` can open the file, but only as the
    /// `permissions` allow; the owner password opens it with all of them.
    /// `permissions` is a collection of the actions to allow: `print`, `modify`,
    /// `copy`, `annotate`, `fill_forms`, `accessibility`, `assemble` and
    /// `print_high_quality`. By default all are allowed. Readers are the ones to
    /// enforce them.
    #[pyfunction]
    #[pyo3(signature = (path, output, *, user_password = "", owner_password = None, permissions = None, encrypt_metadata = true, password = None))]
    #[allow(clippy::too_many_arguments)]
    fn encrypt(
        py: Python<'_>,
        path: Source,
        output: PathBuf,
        user_password: &str,
        owner_password: Option<&str>,
        permissions: Option<Vec<String>>,
        encrypt_metadata: bool,
        password: Option<String>,
    ) -> PyResult<()> {
        let permissions = permission_bits(permissions)?;
        let user = new_password(py, user_password)?;
        let owner = match owner_password {
            Some(owner) => new_password(py, owner)?,
            None => user.clone(),
        };
        let random: Vec<u8> = py
            .import("os")?
            .call_method1("urandom", (RANDOM_BYTES,))?
            .extract()?;
        let password = password_candidates(py, password)?;
        let damaged = py.detach(|| {
            let data = path.load()?;
            let (saved, damaged) = in_new_thread(move || {
                let encryption = Encryption {
                    user_password: &user,
                    owner_password: &owner,
                    permissions,
                    encrypt_metadata,
                };
                encrypt_and_save(&data, password.as_deref(), &encryption, &random)
            })
            .unwrap_or_else(|_| Err(damaged()))?;
            write_file(&output, &saved)?;
            Ok::<_, PyErr>(damaged)
        })?;
        warn_damaged(py, &damaged)
    }

    /// Return what the PDF allows, as a set of the names `encrypt` takes, or
    /// `None` if the PDF isn't protected.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn permissions(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Option<Vec<&'static str>>> {
        let password = password_candidates(py, password)?;
        let bits = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || {
                let document = open_document(&data, password.as_deref(), PROTECTED)?;
                Ok(document.permissions())
            })
            .unwrap_or_else(|_| Err(damaged()))
        })?;
        Ok(bits.map(|bits| {
            PERMISSIONS
                .iter()
                .filter(|(_, bit)| bits & bit != 0)
                .map(|(name, _)| *name)
                .collect()
        }))
    }

    // A `datetime`, aware when the PDF gives its time zone and naive otherwise.
    fn to_datetime(py: Python<'_>, date: Date) -> PyResult<Py<PyAny>> {
        let datetime = py.import("datetime")?;
        let tzinfo = match date.utc_offset {
            Some(minutes) => {
                // timedelta(days, seconds)
                let delta = datetime
                    .getattr("timedelta")?
                    .call1((0, i32::from(minutes) * 60))?;
                datetime.getattr("timezone")?.call1((delta,))?
            }
            None => py.None().into_bound(py),
        };
        let value = datetime.getattr("datetime")?.call1((
            date.year,
            date.month,
            date.day,
            date.hour,
            date.minute,
            date.second,
            0,
            tzinfo,
        ))?;
        Ok(value.unbind())
    }

    fn damaged() -> PyErr {
        PyValueError::new_err("Could not read the PDF: the file may be damaged")
    }

    // A panic in a reader thread is handled: it becomes a ValueError or a PageWarning. So
    // its message isn't printed, as Rust does by default. Panics in other threads still
    // are. The hook belongs to this extension's copy of the standard library, so it
    // doesn't touch other Rust extensions in the same process.
    fn silence_handled_panics() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            let default = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                if thread::current().name() != Some(WORKER) {
                    default(info);
                }
            }));
        });
    }

    // The engine's work runs in a new thread, so a panic, which the engine is
    // fuzzed against, becomes a ValueError instead of a crash, and the stack is
    // large enough for deep files on every platform.
    fn in_new_thread<T, F>(work: F) -> thread::Result<PyResult<T>>
    where
        T: Send + 'static,
        F: FnOnce() -> PyResult<T> + Send + 'static,
    {
        silence_handled_panics();
        thread::Builder::new()
            .name(WORKER.to_string())
            .stack_size(STACK_SIZE)
            .spawn(work)
            .map_err(|e| PyValueError::new_err(format!("Could not read the PDF: {e}")))
            .map_or_else(|e| Ok(Err(e)), |handle| handle.join())
    }

    struct PageFailure {
        index: usize,
        reason: String,
    }

    // The document, opened and unlocked with the password, to read its pages.
    fn unlocked_document<'a>(
        data: &'a [u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<Document<'a>> {
        let mut document = open_document(data, password, PROTECTED)?;
        // An encrypted file whose page tree isn't encrypted opens locked.
        if document.needs_password() {
            let Some(password) = password else {
                return Err(password_error(PROTECTED));
            };
            if !password.iter().any(|candidate| document.unlock(candidate)) {
                return Err(password_error(WRONG_PASSWORD));
            }
        }
        Ok(document)
    }

    // The page at `index` is reported if a problem kept its text from being read.
    fn note_problem(failures: &mut Vec<PageFailure>, index: usize, problem: Option<PageProblem>) {
        let reason = match problem {
            Some(PageProblem::Damaged) => "the page may be damaged",
            Some(PageProblem::UnreadableContent) => "some of the content of the page can't be read",
            None => return,
        };
        failures.push(PageFailure {
            index,
            reason: reason.to_string(),
        });
    }

    // The text of every page, read with the engine. A page that can't be read
    // is left empty and reported, so the other pages keep their text.
    fn extract_page_texts(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<String>, Vec<PageFailure>)> {
        let mut document = unlocked_document(data, password)?;
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        for (index, page) in document_text(&mut document).into_iter().enumerate() {
            note_problem(&mut failures, index, page.problem);
            pages.push(page.text);
        }
        Ok((pages, failures))
    }

    // A box in the default user space of a page, measured from the top left
    // corner of `area`, its visible area: x0, y0, x1, y1.
    fn from_top_left(bbox: Rectangle, area: Rectangle) -> (f64, f64, f64, f64) {
        (
            bbox.left - area.left,
            area.top - bbox.top,
            bbox.right - area.left,
            area.top - bbox.bottom,
        )
    }

    // The visible area of a page, and its blocks.
    type PageOfBlocks = (Rectangle, Vec<TextBlock>);

    // The blocks of every page, and what kept any page from being read.
    fn extract_page_blocks(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<PageOfBlocks>, Vec<PageFailure>)> {
        let mut document = unlocked_document(data, password)?;
        let visible: Vec<Rectangle> = document.pages().iter().map(Page::crop_box).collect();
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        for (index, page) in document_blocks(&mut document).into_iter().enumerate() {
            note_problem(&mut failures, index, page.problem);
            pages.push((visible[index], page.blocks));
        }
        Ok((pages, failures))
    }

    // The words of every page, with their boxes from the top left corner of the
    // visible area of their page. A page that can't be read has no words and
    // is reported.
    fn extract_page_words(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<Word>, Vec<PageFailure>)> {
        let (pages, failures) = extract_page_blocks(data, password)?;
        let mut words = Vec::new();
        for (index, (area, blocks)) in pages.into_iter().enumerate() {
            let mut line = 0;
            for (block, text_block) in blocks.iter().enumerate() {
                for line_words in text_block.words() {
                    line += 1;
                    words.extend(line_words.into_iter().map(|word| {
                        let (x0, y0, x1, y1) = from_top_left(word.bbox, area);
                        Word {
                            page: index + 1,
                            block: block + 1,
                            line,
                            text: word.text,
                            x0,
                            y0,
                            x1,
                            y1,
                        }
                    }));
                }
            }
        }
        Ok((words, failures))
    }

    // The images of every page, with their boxes from the top left corner of
    // the visible area of their page.
    fn extract_page_images(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<Image>, Vec<PageFailure>)> {
        let mut document = unlocked_document(data, password)?;
        let visible: Vec<Rectangle> = document.pages().iter().map(Page::crop_box).collect();
        let mut images = Vec::new();
        let mut failures = Vec::new();
        for (index, page) in document_images(&mut document).into_iter().enumerate() {
            note_problem(&mut failures, index, page.problem);
            for (number, image) in page.images.into_iter().enumerate() {
                let (x0, y0, x1, y1) = from_top_left(image.bbox, visible[index]);
                let format = image.file.as_ref().map(|file| match file.format {
                    ImageFormat::Jpeg => "jpeg",
                    ImageFormat::Jpeg2000 => "jp2",
                    ImageFormat::Png => "png",
                });
                let missing = if image.description.is_none() {
                    "the image is damaged"
                } else {
                    "ozaru can't read this kind of image yet"
                };
                images.push(Image {
                    page: index + 1,
                    number: number + 1,
                    x0,
                    y0,
                    x1,
                    y1,
                    width: image.description.as_ref().map(|d| d.width),
                    height: image.description.as_ref().map(|d| d.height),
                    format,
                    file: image.file.map(|file| file.data),
                    missing,
                });
            }
        }
        Ok((images, failures))
    }

    // The blocks of every page, with their text and their boxes from the top
    // left corner of the visible area of their page.
    fn extract_text_blocks(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<Block>, Vec<PageFailure>)> {
        let (pages, failures) = extract_page_blocks(data, password)?;
        let mut result = Vec::new();
        for (index, (area, blocks)) in pages.into_iter().enumerate() {
            for (number, block) in blocks.into_iter().enumerate() {
                let text = block
                    .lines
                    .iter()
                    .map(|line| line_text(line).trim().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                let (x0, y0, x1, y1) = from_top_left(block.bbox, area);
                result.push(Block {
                    page: index + 1,
                    number: number + 1,
                    text,
                    x0,
                    y0,
                    x1,
                    y1,
                });
            }
        }
        Ok((result, failures))
    }

    // Reports a page whose `what`, such as "text", could not be extracted.
    fn warn(py: Python<'_>, failure: &PageFailure, what: &str) -> PyResult<()> {
        let page = failure.index + 1;
        let message = format!(
            "Could not extract the {what} of page {page}: {}",
            failure.reason
        );
        warn_page(py, page, message, &failure.reason)
    }

    // Emits a PageWarning for `page`, counted from 1.
    fn warn_page(py: Python<'_>, page: usize, message: String, reason: &str) -> PyResult<()> {
        let warning = PageWarning::new_err(message).into_value(py);
        let warning = warning.bind(py);
        warning.setattr("page", page)?;
        warning.setattr("reason", reason)?;
        py.import("warnings")?.call_method1("warn", (warning,))?;
        Ok(())
    }

    /// Return all the text of the PDF.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn extract_text(py: Python<'_>, path: Source, password: Option<String>) -> PyResult<String> {
        let password = password_candidates(py, password)?;
        let (pages, failures) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || extract_page_texts(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure, "text")?;
        }
        Ok(pages.join("\x0c"))
    }

    /// Return every word of the PDF, page by page and line by line, with its box.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn extract_words(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<Word>> {
        let password = password_candidates(py, password)?;
        let (words, failures) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || extract_page_words(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure, "text")?;
        }
        Ok(words)
    }

    /// Return every block of text of the PDF, page by page, with its box.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn extract_blocks(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<Block>> {
        let password = password_candidates(py, password)?;
        let (blocks, failures) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || extract_text_blocks(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure, "text")?;
        }
        Ok(blocks)
    }

    /// Return every image of the PDF, page by page, with its box and its file.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn extract_images(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<Image>> {
        let password = password_candidates(py, password)?;
        let (images, failures) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || extract_page_images(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure, "images")?;
        }
        Ok(images)
    }

    /// Return the document information of the PDF: title, author and dates.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn metadata(py: Python<'_>, path: Source, password: Option<String>) -> PyResult<Metadata> {
        let password = password_candidates(py, password)?;
        let (info, xmp) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || read_info(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        let info = info.unwrap_or_default();
        let date = |date: Option<Date>| date.map(|date| to_datetime(py, date)).transpose();
        // What the information dictionary leaves out comes from XMP.
        let from_xmp = |value: fn(&Xmp) -> Option<String>| xmp.as_ref().and_then(value);
        let xmp_date = |value: fn(&Xmp) -> Option<Date>| xmp.as_ref().and_then(value);
        Ok(Metadata {
            title: info.title().or_else(|| from_xmp(Xmp::title)),
            author: info.author().or_else(|| from_xmp(Xmp::author)),
            subject: info.subject().or_else(|| from_xmp(Xmp::subject)),
            keywords: info.keywords().or_else(|| from_xmp(Xmp::keywords)),
            creator: info.creator().or_else(|| from_xmp(Xmp::creator)),
            producer: info.producer().or_else(|| from_xmp(Xmp::producer)),
            creation_date: date(
                info.creation_date()
                    .or_else(|| xmp_date(Xmp::creation_date)),
            )?,
            modification_date: date(
                info.modification_date()
                    .or_else(|| xmp_date(Xmp::modification_date)),
            )?,
            xmp: xmp.as_ref().map(|xmp| xmp.xml().to_string()),
        })
    }

    /// Return the size and rotation of every page of the PDF.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn page_info(
        py: Python<'_>,
        path: Source,
        password: Option<String>,
    ) -> PyResult<Vec<PageInfo>> {
        let password = password_candidates(py, password)?;
        let (info, damaged_pages) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || read_page_info(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        let reason = "the page may be damaged, so its size and rotation may be wrong";
        for page in damaged_pages {
            warn_page(
                py,
                page,
                format!("Could not read page {page}: {reason}"),
                reason,
            )?;
        }
        Ok(info)
    }

    /// Return the number of pages of the PDF.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn page_count(py: Python<'_>, path: Source, password: Option<String>) -> PyResult<usize> {
        let password = password_candidates(py, password)?;
        py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || count_pages(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })
    }

    // The pages of a document of `count` pages that `pages`, counted from 1,
    // name, from 0; all of them if `pages` is `None`.
    fn page_indexes(pages: Option<Vec<i64>>, count: usize) -> PyResult<Vec<usize>> {
        let Some(pages) = pages else {
            return Ok((0..count).collect());
        };
        pages
            .into_iter()
            .map(|page| {
                usize::try_from(page)
                    .ok()
                    .filter(|&page| (1..=count).contains(&page))
                    .map(|page| page - 1)
                    .ok_or_else(|| {
                        let pages = if count == 1 { "page" } else { "pages" };
                        PyIndexError::new_err(format!(
                            "There is no page {page}: the PDF has {count} {pages}"
                        ))
                    })
            })
            .collect()
    }

    // An engine error while saving, as a Python exception.
    fn save_error(error: ozaru_engine::base::Error) -> PyErr {
        match error.kind() {
            EngineError::PasswordRequired => password_error(PROTECTED),
            EngineError::EncryptedUpdate => PyValueError::new_err(
                "Could not save the changes at the end of the PDF: it is protected with a \
                 password, and ozaru can't add to it yet: save it with incremental=False",
            ),
            EngineError::DamagedUpdate => PyValueError::new_err(
                "Could not save the changes at the end of the PDF: the file is damaged: \
                 save it with incremental=False to repair it",
            ),
            _ => PyValueError::new_err(format!("Could not save the PDF: {error}")),
        }
    }

    // The numbers, from 1, of the damaged pages of `document`, which are
    // saved empty.
    fn damaged_pages(document: &Document<'_>) -> Vec<usize> {
        let pages = document.pages().iter().enumerate();
        pages
            .filter(|(_, page)| page.is_damaged())
            .map(|(index, _)| index + 1)
            .collect()
    }

    // Warns about the damaged pages of a document saved to another file.
    fn warn_damaged(py: Python<'_>, pages: &[usize]) -> PyResult<()> {
        let reason = "the page is damaged, so it was saved empty";
        for &page in pages {
            warn_page(
                py,
                page,
                format!("Could not read page {page}: {reason}"),
                reason,
            )?;
        }
        Ok(())
    }

    // Page `number` of the document, drawn as a PNG file, and whether the page
    // is damaged.
    fn draw_one_page(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
        number: i64,
        scale: f64,
        annotations: bool,
    ) -> PyResult<(Vec<u8>, bool)> {
        let mut document = unlocked_document(data, password)?;
        let index = page_indexes(Some(vec![number]), document.page_count())?[0];
        let page = document
            .page(index)
            .cloned()
            .ok_or_else(|| PyIndexError::new_err(format!("There is no page {number}")))?;
        let limits = document.limits();
        let options = RenderOptions {
            scale,
            annotations,
            ..RenderOptions::default()
        };
        let pixmap =
            draw_page(document.resolver(), &page, &options, limits).map_err(|error| match error
                .kind()
            {
                EngineError::LimitExceeded(_) => PyValueError::new_err(format!(
                    "Page {number} would be too large to draw at scale {scale}: use a smaller scale"
                )),
                _ => PyValueError::new_err(format!("Could not draw page {number}: {error}")),
            })?;
        Ok((pixmap.to_png_level([255, 255, 255], 1), page.is_damaged()))
    }

    /// Draw a page of the PDF as a PNG image, with `scale` pixels for each
    /// point: 1 is 72 dots per inch, and 2 is 144. Annotations and form widgets
    /// are drawn with their appearance unless `annotations` is false.
    #[pyfunction]
    #[pyo3(signature = (path, page, *, scale = 1.0, annotations = true, password = None))]
    fn render_page<'py>(
        py: Python<'py>,
        path: Source,
        page: i64,
        scale: f64,
        annotations: bool,
        password: Option<String>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(PyValueError::new_err(format!(
                "The scale must be a positive number, not {scale}"
            )));
        }
        let password = password_candidates(py, password)?;
        let (png, damaged) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || {
                draw_one_page(&data, password.as_deref(), page, scale, annotations)
            })
            .unwrap_or_else(|_| Err(damaged()))
        })?;
        if damaged {
            let reason = "the page is damaged, so it was drawn empty";
            warn_page(
                py,
                usize::try_from(page).unwrap_or(0),
                format!("Could not read page {page}: {reason}"),
                reason,
            )?;
        }
        Ok(PyBytes::new(py, &png))
    }

    // The document, with the pages at `indexes` turned by `degrees`, saved,
    // and its damaged pages. A page named twice is turned once.
    fn rotate_pages(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
        pages: Option<Vec<i64>>,
        degrees: i64,
        incremental: bool,
    ) -> PyResult<(Vec<u8>, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        let mut indexes = page_indexes(pages, document.page_count())?;
        indexes.sort_unstable();
        indexes.dedup();
        let damaged = damaged_pages(&document);
        let mut editor = Editor::new(&mut document).map_err(save_error)?;
        for index in indexes {
            editor.rotate_page(index, degrees).map_err(save_error)?;
        }
        let saved = if incremental {
            editor.save_incremental()
        } else {
            editor.save()
        };
        Ok((saved.map_err(save_error)?, damaged))
    }

    /// Turn pages of the PDF clockwise and save the result to another file.
    #[pyfunction]
    #[pyo3(signature = (path, output, degrees, *, pages = None, password = None, incremental = false))]
    fn rotate(
        py: Python<'_>,
        path: Source,
        output: PathBuf,
        degrees: i64,
        pages: Option<Vec<i64>>,
        password: Option<String>,
        incremental: bool,
    ) -> PyResult<()> {
        if degrees % 90 != 0 {
            return Err(PyValueError::new_err(format!(
                "Pages can only be turned by a multiple of 90 degrees, not {degrees}"
            )));
        }
        let password = password_candidates(py, password)?;
        let damaged = py.detach(|| {
            let data = path.load()?;
            let (saved, damaged) = in_new_thread(move || {
                rotate_pages(&data, password.as_deref(), pages, degrees, incremental)
            })
            .unwrap_or_else(|_| Err(damaged()))?;
            write_file(&output, &saved)?;
            Ok::<_, PyErr>(damaged)
        })?;
        warn_damaged(py, &damaged)
    }

    // The document with only the pages at `indexes`, in that order, saved,
    // and the damaged pages among them, by their number in the original.
    fn select_and_save(
        document: &mut Document<'_>,
        indexes: &[usize],
    ) -> PyResult<(Vec<u8>, Vec<usize>)> {
        let damaged = indexes
            .iter()
            .filter(|&&index| document.page(index).is_some_and(Page::is_damaged))
            .map(|index| index + 1)
            .collect();
        let mut editor = Editor::new(document).map_err(save_error)?;
        editor.select_pages(indexes).map_err(save_error)?;
        Ok((editor.save().map_err(save_error)?, damaged))
    }

    /// Save some pages of the PDF, in the order given, to another file.
    #[pyfunction]
    #[pyo3(signature = (path, output, pages, *, password = None))]
    fn extract_pages(
        py: Python<'_>,
        path: Source,
        output: PathBuf,
        pages: Vec<i64>,
        password: Option<String>,
    ) -> PyResult<()> {
        if pages.is_empty() {
            return Err(PyValueError::new_err(
                "No pages to save: pages needs at least one page number",
            ));
        }
        let password = password_candidates(py, password)?;
        let damaged = py.detach(|| {
            let data = path.load()?;
            let (saved, damaged) = in_new_thread(move || {
                let mut document = unlocked_document(&data, password.as_deref())?;
                let indexes = page_indexes(Some(pages), document.page_count())?;
                select_and_save(&mut document, &indexes)
            })
            .unwrap_or_else(|_| Err(damaged()))?;
            write_file(&output, &saved)?;
            Ok::<_, PyErr>(damaged)
        })?;
        // A damaged page saved twice is reported once.
        let mut warned = damaged;
        warned.sort_unstable();
        warned.dedup();
        warn_damaged(py, &warned)
    }

    // The name of each part of a PDF split in `parts`: its name with the
    // number of the part, padded so the files sort in order.
    fn part_paths(path: Option<&Path>, folder: &Path, parts: usize) -> Vec<PathBuf> {
        let stem = path
            .and_then(Path::file_stem)
            .map_or_else(|| "part".into(), |stem| stem.to_string_lossy());
        let width = parts.to_string().len();
        (1..=parts)
            .map(|part| folder.join(format!("{stem}-{part:0width$}.pdf")))
            .collect()
    }

    // Saves the document in parts of `size` pages into `folder`, and gives
    // the files written and the damaged pages.
    fn split_document(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
        path: Option<&Path>,
        folder: &Path,
        size: usize,
    ) -> PyResult<(Vec<PathBuf>, Vec<usize>)> {
        let mut document = unlocked_document(data, password)?;
        // The folder is made once the PDF can be read.
        if folder.exists() && !folder.is_dir() {
            return Err(PyNotADirectoryError::new_err((
                ENOTDIR,
                "Not a directory",
                folder.to_string_lossy().into_owned(),
            )));
        }
        fs::create_dir_all(folder).map_err(|e| io_error(e, folder, "create"))?;
        let count = document.page_count();
        let parts = count.div_ceil(size);
        let paths = part_paths(path, folder, parts);
        let mut damaged = Vec::new();
        for (part, output) in paths.iter().enumerate() {
            let first = part * size;
            let indexes: Vec<usize> = (first..count.min(first + size)).collect();
            let (saved, part_damaged) = select_and_save(&mut document, &indexes)?;
            write_file(output, &saved)?;
            damaged.extend(part_damaged);
        }
        Ok((paths, damaged))
    }

    /// Save the PDF as several files in a folder, one for each page or group of pages.
    #[pyfunction]
    #[pyo3(signature = (path, folder, *, pages_per_file = 1, password = None))]
    fn split(
        py: Python<'_>,
        path: Source,
        folder: PathBuf,
        pages_per_file: i64,
        password: Option<String>,
    ) -> PyResult<Vec<PathBuf>> {
        let Some(size) = usize::try_from(pages_per_file)
            .ok()
            .filter(|&size| size > 0)
        else {
            return Err(PyValueError::new_err(format!(
                "pages_per_file must be 1 or more, not {pages_per_file}"
            )));
        };
        let password = password_candidates(py, password)?;
        let (paths, damaged) = py.detach(|| {
            let data = path.load()?;
            in_new_thread(move || split_document(&data, password.as_deref(), path.name(), &folder, size))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        warn_damaged(py, &damaged)?;
        Ok(paths)
    }

    // An error opening one of several files, with its path first in the
    // message.
    fn in_file(error: PyErr, label: &str) -> PyErr {
        Python::attach(|py| {
            let message = format!("{label}: {}", error.value(py));
            if error.is_instance_of::<PasswordError>(py) {
                PasswordError::new_err(message)
            } else {
                PyValueError::new_err(message)
            }
        })
    }

    // The documents of `files`, joined in order and saved, and the numbers of
    // their damaged pages in the joined document.
    fn merge_documents(
        files: &[(String, Arc<Vec<u8>>)],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<u8>, Vec<usize>)> {
        let mut documents = Vec::with_capacity(files.len());
        for (label, data) in files {
            // The password is for the files that need one: an encrypted file
            // that opens without it is opened so.
            let document = match unlocked_document(data, None) {
                Err(error)
                    if password.is_some()
                        && Python::attach(|py| error.is_instance_of::<PasswordError>(py)) =>
                {
                    unlocked_document(data, password)
                }
                result => result,
            };
            documents.push(document.map_err(|e| in_file(e, label))?);
        }
        let mut damaged = Vec::new();
        let mut before = 0;
        for document in &documents {
            damaged.extend(
                damaged_pages(document)
                    .into_iter()
                    .map(|page| page + before),
            );
            before += document.page_count();
        }
        let (first, others) = documents.split_first_mut().expect("there is always a file");
        let mut editor = Editor::new(first).map_err(save_error)?;
        for other in others {
            let pages: Vec<usize> = (0..other.page_count()).collect();
            editor.import_pages(other, &pages).map_err(save_error)?;
        }
        Ok((editor.save().map_err(save_error)?, damaged))
    }

    /// Join several PDFs into one file, in the order given.
    #[pyfunction]
    #[pyo3(signature = (paths, output, *, password = None))]
    fn merge(
        py: Python<'_>,
        paths: Vec<Source>,
        output: PathBuf,
        password: Option<String>,
    ) -> PyResult<()> {
        if paths.is_empty() {
            return Err(PyValueError::new_err(
                "No PDFs to join: paths needs at least one file",
            ));
        }
        let password = password_candidates(py, password)?;
        let damaged = py.detach(|| {
            let files = paths
                .into_iter()
                .map(|source| source.load().map(|data| (source.label(), data)))
                .collect::<PyResult<Vec<_>>>()?;
            let (saved, damaged) =
                in_new_thread(move || merge_documents(&files, password.as_deref()))
                    .unwrap_or_else(|_| Err(damaged()))?;
            write_file(&output, &saved)?;
            Ok::<_, PyErr>(damaged)
        })?;
        warn_damaged(py, &damaged)
    }
}
