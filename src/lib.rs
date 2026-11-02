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

    use pdf_oxide::document::PdfDocument;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
    use pyo3::prelude::*;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;

    const STACK_SIZE: usize = 8 * 1024 * 1024;

    // Internal helper (not visible from Python): opens the PDF or returns a clear error.
    fn open(path: &Path) -> PyResult<PdfDocument> {
        if !path.exists() {
            return Err(PyFileNotFoundError::new_err(format!(
                "File not found: {}",
                path.display()
            )));
        }
        PdfDocument::open(path)
            .map_err(|e| PyValueError::new_err(format!("Could not open the PDF: {e}")))
    }

    fn damaged() -> PyErr {
        PyValueError::new_err("Could not read the PDF: the file may be damaged")
    }

    // pdf_oxide keeps per-thread state that a panic leaves dirty, which silently loses text
    // from later PDFs. Running the work in a new thread keeps that state isolated.
    fn in_new_thread<T, F>(work: F) -> thread::Result<PyResult<T>>
    where
        T: Send + 'static,
        F: FnOnce() -> PyResult<T> + Send + 'static,
    {
        thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(work)
            .map_err(|e| PyValueError::new_err(format!("Could not read the PDF: {e}")))
            .map_or_else(|e| Ok(Err(e)), |handle| handle.join())
    }

    fn read<T, F>(path: PathBuf, operation: F) -> PyResult<T>
    where
        T: Send + 'static,
        F: FnOnce(PdfDocument) -> PyResult<T> + Send + 'static,
    {
        in_new_thread(move || operation(open(&path)?)).unwrap_or_else(|_| Err(damaged()))
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
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        let mut page_count = None;

        loop {
            let start = pages.len();
            let (sender, receiver) = mpsc::channel();
            let path = path.clone();
            let outcome = in_new_thread(move || {
                let doc = open(&path)?;
                if !doc.is_authenticated() {
                    return Err(PyValueError::new_err("The PDF is protected with a password"));
                }
                let count = doc
                    .page_count()
                    .map_err(|e| PyValueError::new_err(format!("Could not count the pages: {e}")))?;
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
                        failures.push(PageFailure { index: pages.len(), reason });
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
            read(path, |doc| {
                doc.page_count()
                    .map_err(|e| PyValueError::new_err(format!("Could not count the pages: {e}")))
            })
        })
    }
}
