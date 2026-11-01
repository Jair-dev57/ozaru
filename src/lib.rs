use pyo3::prelude::*;

#[pymodule]
mod ozaru {
    use pdf_oxide::document::PdfDocument;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
    use pyo3::prelude::*;
    use std::path::{Path, PathBuf};

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

    /// Return all the text of the PDF.
    #[pyfunction]
    fn extract_text(path: PathBuf) -> PyResult<String> {
        let doc = open(&path)?;
        doc.extract_all_text()
            .map_err(|e| PyValueError::new_err(format!("Could not extract the text: {e}")))
    }

    /// Return the number of pages of the PDF.
    #[pyfunction]
    fn page_count(path: PathBuf) -> PyResult<usize> {
        let doc = open(&path)?;
        doc.page_count()
            .map_err(|e| PyValueError::new_err(format!("Could not count the pages: {e}")))
    }
}
