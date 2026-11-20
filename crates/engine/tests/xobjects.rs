use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::content::{Interpreter, PageContent};
use ozaru_engine::{Document, OpenOptions};

mod common;
use common::{file_with, stream_object};

const HELVETICA: &str = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";
const COURIER: &str = "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>";

// A page with `content`, Helvetica as /F1, the XObjects in `xobjects`, and
// `objects` from number 10 on.
fn file(content: &str, xobjects: &str, objects: &[(u32, String)]) -> Vec<u8> {
    let mut all = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> \
                 /XObject << {xobjects} >> >> /Contents 5 0 R >>"
            ),
        ),
        (4, HELVETICA.to_string()),
        (5, stream_object("", content)),
    ];
    all.extend_from_slice(objects);
    file_with(&all)
}

fn form(entries: &str, content: &str) -> String {
    stream_object(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 1000 1000] {entries}"),
        content,
    )
}

fn run_with(file: &[u8], limits: Limits) -> PageContent {
    let options = OpenOptions {
        limits,
        ..OpenOptions::default()
    };
    let mut document = Document::open_with(file, options).unwrap();
    let page = document.page(0).unwrap().clone();
    Interpreter::new(limits).run_page(document.resolver(), &page)
}

fn run(file: &[u8]) -> PageContent {
    run_with(file, Limits::default())
}

fn kinds(content: &PageContent) -> Vec<DiagnosticKind> {
    content.diagnostics.iter().map(|d| d.kind()).collect()
}

fn text(content: &PageContent) -> String {
    content
        .glyphs
        .iter()
        .filter_map(|g| g.text.as_deref())
        .collect()
}

fn origins(content: &PageContent) -> Vec<(f64, f64)> {
    content
        .glyphs
        .iter()
        .map(|g| {
            let (x, y) = g.origin();
            ((x * 1000.0).round() / 1000.0, (y * 1000.0).round() / 1000.0)
        })
        .collect()
}

#[test]
fn forms_are_painted_with_their_matrix() {
    let file = file(
        "q 1 0 0 1 100 200 cm /X1 Do Q BT /F1 10 Tf (B) Tj ET",
        "/X1 10 0 R",
        &[(
            10,
            form("/Matrix [2 0 0 2 10 0]", "BT /F1 10 Tf 5 5 Td (A) Tj ET"),
        )],
    );
    let content = run(&file);
    assert!(content.diagnostics.is_empty(), "{:?}", kinds(&content));
    assert_eq!(text(&content), "AB");
    // (5, 5) scaled by 2, moved by (10, 0) and then by (100, 200).
    assert_eq!(origins(&content), [(120.0, 210.0), (0.0, 0.0)]);
    assert_eq!(content.glyphs[0].matrix.a, 20.0);
}

#[test]
fn forms_have_their_own_resources_or_those_of_the_page() {
    let file = file(
        "/X1 Do /X2 Do",
        "/X1 10 0 R /X2 11 0 R",
        &[
            (
                10,
                form(
                    "/Resources << /Font << /F1 12 0 R >> >>",
                    "BT /F1 10 Tf (A) Tj ET",
                ),
            ),
            // No /Resources: the page's are used, as older files expect.
            (11, form("", "BT /F1 10 Tf (B) Tj ET")),
            (12, COURIER.to_string()),
        ],
    );
    let content = run(&file);
    assert_eq!(text(&content), "AB");
    let fonts: Vec<_> = content
        .glyphs
        .iter()
        .map(|g| g.font.base_font().unwrap().as_bytes().to_vec())
        .collect();
    assert_eq!(fonts, [b"Courier".to_vec(), b"Helvetica".to_vec()]);
}

#[test]
fn forms_can_paint_other_forms() {
    let file = file(
        "/X1 Do",
        "/X1 10 0 R",
        &[
            (
                10,
                form(
                    "/Matrix [1 0 0 1 100 0] /Resources << /XObject << /X2 11 0 R >> \
                     /Font << /F1 4 0 R >> >>",
                    "/X2 Do /X2 Do",
                ),
            ),
            (
                11,
                form(
                    "/Matrix [1 0 0 1 0 50] /Resources << /Font << /F1 4 0 R >> >>",
                    "BT /F1 10 Tf (A) Tj ET",
                ),
            ),
        ],
    );
    let content = run(&file);
    assert!(content.diagnostics.is_empty(), "{:?}", kinds(&content));
    // The same form twice, in the same place.
    assert_eq!(origins(&content), [(100.0, 50.0), (100.0, 50.0)]);
}

#[test]
fn a_form_that_paints_itself_is_painted_once() {
    let file = file(
        "/X1 Do",
        "/X1 10 0 R",
        &[
            (
                10,
                form(
                    "/Resources << /XObject << /X1 10 0 R /X2 11 0 R >> /Font << /F1 4 0 R >> >>",
                    "BT /F1 10 Tf (A) Tj ET /X1 Do /X2 Do",
                ),
            ),
            (
                11,
                form("/Resources << /XObject << /X1 10 0 R >> >>", "/X1 Do"),
            ),
        ],
    );
    let content = run(&file);
    assert_eq!(text(&content), "A");
    assert_eq!(
        kinds(&content),
        [DiagnosticKind::XObjectCycle, DiagnosticKind::XObjectCycle]
    );
}

#[test]
fn forms_nested_too_deeply_are_not_painted() {
    // Forms 10 to 59, each painting the next, and the last showing text.
    let mut objects = Vec::new();
    for number in 10..60 {
        let content = if number == 59 {
            "BT /F1 10 Tf (A) Tj ET".to_string()
        } else {
            "/X Do".to_string()
        };
        objects.push((
            number,
            form(
                &format!("/Resources << /XObject << /X {} 0 R >> >>", number + 1),
                &content,
            ),
        ));
    }
    let content = run(&file("/X Do", "/X 10 0 R", &objects));
    assert!(content.glyphs.is_empty());
    assert_eq!(kinds(&content), [DiagnosticKind::XObjectTooDeep]);
}

#[test]
fn painting_a_form_many_times_stops_at_the_limit() {
    // Each form paints the next one ten times: 10^6 copies of the text.
    let mut objects = Vec::new();
    for number in 10..16 {
        let content = if number == 15 {
            "BT /F1 10 Tf (A) Tj ET".to_string()
        } else {
            "/X Do ".repeat(10)
        };
        objects.push((
            number,
            form(
                &format!(
                    "/Resources << /XObject << /X {} 0 R >> /Font << /F1 4 0 R >> >>",
                    number + 1
                ),
                &content,
            ),
        ));
    }
    let limits = Limits {
        max_content_operations: 10_000,
        ..Limits::default()
    };
    let content = run_with(&file("/X Do", "/X 10 0 R", &objects), limits);
    assert!(content.glyphs.len() < 10_000);
    assert_eq!(kinds(&content), [DiagnosticKind::ContentLimitReached]);
}

#[test]
fn images_are_not_forms() {
    let image = stream_object(
        "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 \
         /ColorSpace /DeviceGray",
        "\u{0}",
    );
    let content = run(&file("/Im1 Do", "/Im1 10 0 R", &[(10, image)]));
    assert!(content.diagnostics.is_empty());
    assert!(content.glyphs.is_empty());
}

#[test]
fn missing_xobjects_are_reported() {
    let content = run(&file(
        "/X9 Do /X1 Do /X2 Do 5 Do",
        "/X1 10 0 R /X2 11 0 R",
        &[
            (10, "<< /Subtype /Form >>".to_string()),
            (11, stream_object("/Subtype /Other", "")),
        ],
    ));
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::MissingResource,
            DiagnosticKind::MissingResource,
            DiagnosticKind::MissingResource,
            DiagnosticKind::InvalidOperands
        ]
    );
}

#[test]
fn invalid_form_entries_use_the_defaults() {
    let file = file(
        "/X1 Do",
        "/X1 10 0 R",
        &[(
            10,
            form(
                "/Matrix [1 2] /Resources 7",
                "BT /F1 10 Tf 1 1 Td (A) Tj ET",
            ),
        )],
    );
    let content = run(&file);
    assert_eq!(origins(&content), [(1.0, 1.0)]);
    assert_eq!(
        kinds(&content),
        [DiagnosticKind::InvalidForm, DiagnosticKind::InvalidForm]
    );
}

#[test]
fn the_state_after_a_form_is_the_one_before() {
    // The form changes the matrix, the font and the text state, and leaves a
    // state saved and a text object open.
    let file = file(
        "BT /F1 10 Tf ET /X1 Do BT 10 10 Td (A) Tj ET",
        "/X1 10 0 R",
        &[
            (
                10,
                form(
                    "/Resources << /Font << /F2 11 0 R >> >>",
                    "2 0 0 2 0 0 cm q BT /F2 20 Tf 5 Tc",
                ),
            ),
            (11, COURIER.to_string()),
        ],
    );
    let content = run(&file);
    assert_eq!(content.glyphs.len(), 1);
    let glyph = &content.glyphs[0];
    assert_eq!(glyph.font.base_font().unwrap().as_bytes(), b"Helvetica");
    assert_eq!(glyph.font_size, 10.0);
    assert_eq!(glyph.origin(), (10.0, 10.0));
    assert_eq!(glyph.matrix.a, 10.0);
}
