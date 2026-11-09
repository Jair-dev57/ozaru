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
    "Warns that the text of one page could not be extracted."
);

#[pymodule]
mod ozaru {
    #[pymodule_export]
    use super::PageWarning;
    #[pymodule_export]
    use super::PasswordError;

    use ozaru_engine::base::ErrorKind as EngineError;
    use ozaru_engine::base::{Diagnostics, Limits};
    use ozaru_engine::document::{Date, Info, Page, Rectangle};
    use ozaru_engine::text::{PageProblem, document_text, document_words};
    use ozaru_engine::xref::Resolver;
    use ozaru_engine::{Document, OpenOptions};
    use pyo3::exceptions::{
        PyFileNotFoundError, PyIsADirectoryError, PyNotADirectoryError, PyOSError,
        PyPermissionError, PyValueError,
    };
    use pyo3::prelude::*;
    use pyo3::types::{PyString, PyTuple};
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
            _ => PyOSError::new_err(format!("Could not read {}: {error}", path.display())),
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
    fn read_info(data: &[u8], password: Option<&[Vec<u8>]>) -> PyResult<Option<Info>> {
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
        Ok(ozaru_engine::document::read_info(
            &mut resolver,
            &mut diagnostics,
        ))
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
                "Word(page={}, line={}, text={text}, x0={:.2}, y0={:.2}, x1={:.2}, y1={:.2})",
                self.page, self.line, self.x0, self.y0, self.x1, self.y1
            ))
        }
    }

    impl Word {
        fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
            (
                self.page,
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
    fn extract_pages(
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

    // The words of every page, with their boxes from the top left corner of the
    // visible area of their page. A page that can't be read has no words and
    // is reported.
    fn extract_page_words(
        data: &[u8],
        password: Option<&[Vec<u8>]>,
    ) -> PyResult<(Vec<Word>, Vec<PageFailure>)> {
        let mut document = unlocked_document(data, password)?;
        let visible: Vec<Rectangle> = document.pages().iter().map(Page::crop_box).collect();
        let mut words = Vec::new();
        let mut failures = Vec::new();
        for (index, page) in document_words(&mut document).into_iter().enumerate() {
            note_problem(&mut failures, index, page.problem);
            let area = visible[index];
            for (line, line_words) in page.lines.into_iter().enumerate() {
                words.extend(line_words.into_iter().map(|word| Word {
                    page: index + 1,
                    line: line + 1,
                    text: word.text,
                    x0: word.bbox.left - area.left,
                    y0: area.top - word.bbox.top,
                    x1: word.bbox.right - area.left,
                    y1: area.top - word.bbox.bottom,
                }));
            }
        }
        Ok((words, failures))
    }

    fn warn(py: Python<'_>, failure: &PageFailure) -> PyResult<()> {
        let page = failure.index + 1;
        let message = format!(
            "Could not extract the text of page {page}: {}",
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
    fn extract_text(py: Python<'_>, path: PathBuf, password: Option<String>) -> PyResult<String> {
        let password = password_candidates(py, password)?;
        let (pages, failures) = py.detach(|| {
            let data = read_file(&path)?;
            in_new_thread(move || extract_pages(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure)?;
        }
        Ok(pages.join("\x0c"))
    }

    /// Return every word of the PDF, page by page and line by line, with its box.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn extract_words(
        py: Python<'_>,
        path: PathBuf,
        password: Option<String>,
    ) -> PyResult<Vec<Word>> {
        let password = password_candidates(py, password)?;
        let (words, failures) = py.detach(|| {
            let data = read_file(&path)?;
            in_new_thread(move || extract_page_words(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        for failure in &failures {
            warn(py, failure)?;
        }
        Ok(words)
    }

    /// Return the document information of the PDF: title, author and dates.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn metadata(py: Python<'_>, path: PathBuf, password: Option<String>) -> PyResult<Metadata> {
        let password = password_candidates(py, password)?;
        let info = py.detach(|| {
            let data = read_file(&path)?;
            in_new_thread(move || read_info(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })?;
        let info = info.unwrap_or_default();
        let date = |date: Option<Date>| date.map(|date| to_datetime(py, date)).transpose();
        Ok(Metadata {
            title: info.title(),
            author: info.author(),
            subject: info.subject(),
            keywords: info.keywords(),
            creator: info.creator(),
            producer: info.producer(),
            creation_date: date(info.creation_date())?,
            modification_date: date(info.modification_date())?,
        })
    }

    /// Return the size and rotation of every page of the PDF.
    #[pyfunction]
    #[pyo3(signature = (path, *, password = None))]
    fn page_info(
        py: Python<'_>,
        path: PathBuf,
        password: Option<String>,
    ) -> PyResult<Vec<PageInfo>> {
        let password = password_candidates(py, password)?;
        let (info, damaged_pages) = py.detach(|| {
            let data = read_file(&path)?;
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
    fn page_count(py: Python<'_>, path: PathBuf, password: Option<String>) -> PyResult<usize> {
        let password = password_candidates(py, password)?;
        py.detach(|| {
            let data = read_file(&path)?;
            in_new_thread(move || count_pages(&data, password.as_deref()))
                .unwrap_or_else(|_| Err(damaged()))
        })
    }
}
