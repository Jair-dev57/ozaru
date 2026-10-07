use pyo3::prelude::*;

#[pymodule]
mod ozaru {
    use pdf_oxide::document::PdfDocument;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
    use pyo3::prelude::*;
    use std::path::Path;

    // Función interna (no visible desde Python): abre el PDF o devuelve un error claro.
    fn abrir(path: &str) -> PyResult<PdfDocument> {
        if !Path::new(path).exists() {
            return Err(PyFileNotFoundError::new_err(format!(
                "No existe el archivo: {path}"
            )));
        }
        PdfDocument::open(path)
            .map_err(|e| PyValueError::new_err(format!("No se pudo abrir el PDF: {e}")))
    }

    /// Devuelve todo el texto del PDF.
    #[pyfunction]
    fn extract_text(path: &str) -> PyResult<String> {
        let doc = abrir(path)?;
        doc.extract_all_text()
            .map_err(|e| PyValueError::new_err(format!("Error al extraer el texto: {e}")))
    }

    /// Devuelve el número de páginas del PDF.
    #[pyfunction]
    fn page_count(path: &str) -> PyResult<usize> {
        let doc = abrir(path)?;
        doc.page_count()
            .map_err(|e| PyValueError::new_err(format!("Error al contar las páginas: {e}")))
    }
}