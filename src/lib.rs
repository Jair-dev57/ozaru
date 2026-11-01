use pyo3::prelude::*;

#[pymodule]
mod ozaru {
    use pdf_oxide::document::PdfDocument;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
    use pyo3::prelude::*;
    use std::path::{Path, PathBuf};
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

    // pdf_oxide keeps per-thread state that a panic leaves dirty, which silently loses text
    // from later PDFs. A thread per call keeps that state isolated and contains the panic.
    fn read<T, F>(path: PathBuf, operation: F) -> PyResult<T>
    where
        T: Send + 'static,
        F: FnOnce(PdfDocument) -> PyResult<T> + Send + 'static,
    {
        thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(move || operation(open(&path)?))
            .map_err(|e| PyValueError::new_err(format!("Could not read the PDF: {e}")))?
            .join()
            .unwrap_or_else(|_| {
                Err(PyValueError::new_err(
                    "Could not read the PDF: the file may be damaged",
                ))
            })
    }

    /// Return all the text of the PDF.
    #[pyfunction]
    fn extract_text(py: Python<'_>, path: PathBuf) -> PyResult<String> {
        py.detach(|| {
            read(path, |doc| {
                doc.extract_all_text()
                    .map_err(|e| PyValueError::new_err(format!("Could not extract the text: {e}")))
            })
        })
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
