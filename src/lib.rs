use pyo3::prelude::*;

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

    use ozaru_engine::Document;
    use ozaru_engine::base::ErrorKind as EngineError;
    use pdf_oxide::document::PdfDocument;
    use pyo3::exceptions::{
        PyFileNotFoundError, PyIsADirectoryError, PyNotADirectoryError, PyOSError,
        PyPermissionError, PyValueError,
    };
    use pyo3::prelude::*;
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Once, mpsc};
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

    // Reads the whole file, as pdf_oxide's own open() does, so file errors are ours to report.
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

    fn open(data: &[u8]) -> PyResult<PdfDocument> {
        PdfDocument::from_bytes(data.to_vec())
            .map_err(|e| PyValueError::new_err(format!("Could not open the PDF: {e}")))
    }

    // Counts the pages with the engine. Encrypted files are counted without their
    // password, since the page tree can usually be read anyway.
    fn count_pages(data: &[u8]) -> PyResult<usize> {
        let error = match Document::open(data) {
            Ok(document) => return Ok(document.page_count()),
            Err(error) => error,
        };
        let message = match error.kind() {
            EngineError::PasswordRequired => {
                "Could not count the pages: the PDF is protected with a password".to_string()
            }
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

    // pdf_oxide keeps per-thread state that a panic leaves dirty, which silently loses text
    // from later PDFs. Running the work in a new thread keeps that state isolated. The
    // engine has no such state, but its work runs in a new thread too, so a panic, which
    // the engine is fuzzed against, becomes a ValueError instead of a crash.
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

    enum Progress {
        PageCount(usize),
        Page(Result<String, String>),
    }

    struct PageFailure {
        index: usize,
        reason: String,
    }

    // Reads every page. A page that fails is left empty and reported, so the other pages
    // keep their text. After a panic, reading resumes in a new thread at the next page.
    fn extract_pages(path: PathBuf) -> PyResult<(Vec<String>, Vec<PageFailure>)> {
        let data = read_file(&path)?;
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        let mut page_count = None;

        loop {
            let start = pages.len();
            let (sender, receiver) = mpsc::channel();
            let data = Arc::clone(&data);
            let outcome = in_new_thread(move || {
                let doc = open(&data)?;
                if !doc.is_authenticated() {
                    return Err(PyValueError::new_err(
                        "The PDF is protected with a password",
                    ));
                }
                let count = doc.page_count().map_err(|e| {
                    PyValueError::new_err(format!("Could not count the pages: {e}"))
                })?;
                let _ = sender.send(Progress::PageCount(count));
                for index in start..count {
                    let _ = sender.send(Progress::Page(
                        doc.extract_text(index).map_err(|e| e.to_string()),
                    ));
                }
                Ok(())
            });

            for progress in receiver.try_iter() {
                match progress {
                    Progress::PageCount(count) => page_count = Some(count),
                    Progress::Page(Ok(text)) => pages.push(text),
                    Progress::Page(Err(reason)) => {
                        failures.push(PageFailure {
                            index: pages.len(),
                            reason,
                        });
                        pages.push(String::new());
                    }
                }
            }

            match (outcome, page_count) {
                (Ok(result), _) => return result.map(|()| (pages, failures)),
                // The panic happened before the pages could be counted.
                (Err(_), None) => return Err(damaged()),
                (Err(_), Some(count)) => {
                    failures.push(PageFailure {
                        index: pages.len(),
                        reason: "the page may be damaged".to_string(),
                    });
                    pages.push(String::new());
                    if pages.len() >= count {
                        return Ok((pages, failures));
                    }
                }
            }
        }
    }

    fn warn(py: Python<'_>, failure: &PageFailure) -> PyResult<()> {
        let page = failure.index + 1;
        let warning = PageWarning::new_err(format!(
            "Could not extract the text of page {page}: {}",
            failure.reason
        ))
        .into_value(py);
        let warning = warning.bind(py);
        warning.setattr("page", page)?;
        warning.setattr("reason", &failure.reason)?;
        py.import("warnings")?.call_method1("warn", (warning,))?;
        Ok(())
    }

    /// Return all the text of the PDF.
    #[pyfunction]
    fn extract_text(py: Python<'_>, path: PathBuf) -> PyResult<String> {
        let (pages, failures) = py.detach(|| extract_pages(path))?;
        for failure in &failures {
            warn(py, failure)?;
        }
        Ok(pages.join("\x0c"))
    }

    /// Return the number of pages of the PDF.
    #[pyfunction]
    fn page_count(py: Python<'_>, path: PathBuf) -> PyResult<usize> {
        py.detach(|| {
            let data = read_file(&path)?;
            in_new_thread(move || count_pages(&data)).unwrap_or_else(|_| Err(damaged()))
        })
    }
}
