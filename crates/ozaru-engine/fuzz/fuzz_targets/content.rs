#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::content::{ContentParser, Interpreter};
use ozaru_engine::document::Resources;
use ozaru_engine::text::{document_text, page_text};
use ozaru_engine::{Document, OpenOptions};

// Reads the input as a content stream, then runs it as the content of a page
// with a simple and a composite font, a form and marked content properties.
// Every operation and diagnostic is inside the input, and every glyph is
// placed at a finite position.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_content_operations: 100_000,
        ..Limits::default()
    };
    let mut parser = ContentParser::new(data, limits);
    while let Some(operation) = parser.next_operation() {
        assert!(operation.offset < data.len());
        if let Some(image) = operation.inline_image {
            assert!(image.data.len() <= data.len());
        }
    }
    assert!(parser.diagnostics().iter().all(|d| d.offset() <= data.len()));

    let file = page_with(data);
    let options = OpenOptions {
        limits,
        password: None,
    };
    let Ok(mut document) = Document::open_with(&file, options) else {
        return;
    };
    let Some(page) = document.page(0).cloned() else {
        return;
    };
    let mut interpreter = Interpreter::new(limits);
    let content = interpreter.run_page(document.resolver(), &page);
    assert!(content.glyphs.len() <= 100_000);
    for glyph in &content.glyphs {
        assert!(glyph.matrix.is_finite());
        assert!(glyph.width.is_finite());
        if let Some(index) = glyph.marked_content {
            assert!(index < content.marked_content.len());
        }
        let _ = content.marked_content_of(glyph).count();
    }
    assert!(content.diagnostics.iter().all(|d| d.offset() <= file.len()));
    // The text of the page, and of the whole document.
    let _ = page_text(&content, &page.crop_box());
    let _ = document_text(&mut document);
    let _ = interpreter.run_content(document.resolver(), data, &Resources::default(), 0);
});

// A file whose page has `content` and resources for it to use. It has no
// cross-reference table, so opening it rebuilds one.
fn page_with(content: &[u8]) -> Vec<u8> {
    let mut file = b"%PDF-1.7\n".to_vec();
    let objects = [
        "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj",
        "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj",
        "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /F1 4 0 R /F2 5 0 R >> /XObject << /X1 7 0 R >> \
         /Properties << /P1 << /MCID 1 /ActualText (fi) >> >> \
         /ExtGState << /G1 << /Font [4 0 R 12] >> >> >> /Contents 6 0 R >> endobj",
        "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj",
        "5 0 obj << /Type /Font /Subtype /Type0 /BaseFont /Noto /Encoding /Identity-V \
         /DescendantFonts [<< /Subtype /CIDFontType2 /DW 500 /W2 [1 [-900 250 880]] >>] >> endobj",
        "7 0 obj << /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix [1 0 0 1 5 5] \
         /Length 31 >>\nstream\nBT /F1 9 Tf (form) Tj ET /X1 Do\nendstream endobj",
    ];
    for object in objects {
        file.extend_from_slice(object.as_bytes());
        file.push(b'\n');
    }
    file.extend_from_slice(format!("6 0 obj << /Length {} >>\nstream\n", content.len()).as_bytes());
    file.extend_from_slice(content);
    file.extend_from_slice(b"\nendstream endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n");
    file
}
