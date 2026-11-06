use std::collections::HashMap;
use std::path::Path;

use ozaru_engine::base::{DiagnosticKind, Diagnostics, ErrorKind, Limit, Limits};
use ozaru_engine::document::{read_catalog, read_pages};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// A file whose objects are all listed in one table, with object 1 as `/Root`.
// It returns the file and the offset of each object, by number.
fn file(objects: &[(u32, &str)]) -> (Vec<u8>, HashMap<u32, usize>) {
    let mut pdf = Pdf::new();
    let offsets: HashMap<u32, usize> = objects
        .iter()
        .map(|&(number, body)| (number, pdf.object(number, body)))
        .collect();
    let mut table: Vec<(u32, XrefEntry)> = offsets
        .iter()
        .map(|(&number, &offset)| {
            let entry = XrefEntry::InUse {
                offset,
                generation: 0,
            };
            (number, entry)
        })
        .collect();
    table.sort_by_key(|&(number, _)| number);
    let start = pdf.table(&table, "/Size 100000 /Root 1 0 R");
    pdf.end(start);
    (pdf.file, offsets)
}

type Pages = Result<Vec<Option<u32>>, ErrorKind>;

// The object number of each page, and the diagnostics of the page tree.
fn pages_with(file: &[u8], limits: Limits) -> (Pages, Vec<(DiagnosticKind, usize)>) {
    let mut resolver = Resolver::open(file, limits).unwrap();
    let mut diagnostics = Diagnostics::new(100);
    let catalog = read_catalog(&mut resolver, &mut diagnostics).unwrap();
    let pages = read_pages(&mut resolver, &catalog, limits, &mut diagnostics)
        .map(|pages| pages.iter().map(|p| p.id().map(|id| id.number)).collect())
        .map_err(|e| e.kind());
    let diagnostics = diagnostics.iter().map(|d| (d.kind(), d.offset())).collect();
    (pages, diagnostics)
}

fn pages_of(file: &[u8]) -> (Pages, Vec<(DiagnosticKind, usize)>) {
    pages_with(file, Limits::default())
}

const BOX: &str = "/MediaBox [0 0 612 792]";
const CATALOG: (u32, &str) = (1, "<< /Type /Catalog /Pages 2 0 R >>");

fn page(number: u32) -> (u32, String) {
    (number, format!("<< /Type /Page /Parent 2 0 R {BOX} >>"))
}

fn objects(list: &[(u32, String)]) -> Vec<(u32, &str)> {
    list.iter()
        .map(|(number, body)| (*number, body.as_str()))
        .collect()
}

#[test]
fn the_pages_of_a_flat_tree_are_in_order() {
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [5 0 R 3 0 R 4 0 R] /Count 3 >>".to_string(),
        ),
    ];
    list.extend([page(3), page(4), page(5)]);
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(5), Some(3), Some(4)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn the_pages_of_an_unbalanced_tree_are_in_order() {
    // 2 ── 3
    //  └── 6 ── 7
    //       └── 8 ── 9
    //            └── 10
    //  └── 11
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 6 0 R 11 0 R] /Count 5 >>".to_string(),
        ),
        (
            6,
            "<< /Type /Pages /Kids [7 0 R 8 0 R] /Count 3 >>".to_string(),
        ),
        (
            8,
            "<< /Type /Pages /Kids [9 0 R 10 0 R] /Count 2 >>".to_string(),
        ),
    ];
    list.extend([page(3), page(7), page(9), page(10), page(11)]);
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(
        pages.unwrap(),
        [Some(3), Some(7), Some(9), Some(10), Some(11)]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn a_very_deep_tree_does_not_overflow_the_stack() {
    const DEPTH: u32 = 20_000;
    let mut list = vec![(1, CATALOG.1.to_string())];
    for number in 2..DEPTH {
        list.push((
            number,
            format!("<< /Type /Pages /Kids [{} 0 R] /Count 1 >>", number + 1),
        ));
    }
    list.push(page(DEPTH));
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(DEPTH)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn a_node_that_contains_its_ancestor_is_skipped() {
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".to_string(),
        ),
        (
            6,
            "<< /Type /Pages /Kids [2 0 R 4 0 R] /Count 1 >>".to_string(),
        ),
    ];
    list.extend([page(3), page(4)]);
    let (file, offsets) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(3), Some(4)]);
    assert_eq!(diagnostics, [(DiagnosticKind::PageTreeCycle, offsets[&2])]);
}

#[test]
fn a_node_that_contains_itself_is_skipped() {
    let list = vec![
        (1, CATALOG.1.to_string()),
        (2, "<< /Type /Pages /Kids [2 0 R] /Count 0 >>".to_string()),
    ];
    let (file, offsets) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), []);
    assert_eq!(diagnostics, [(DiagnosticKind::PageTreeCycle, offsets[&2])]);
}

#[test]
fn a_page_listed_twice_is_kept_once() {
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 3 0 R] /Count 2 >>".to_string(),
        ),
    ];
    list.push(page(3));
    let (file, offsets) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(3)]);
    assert_eq!(
        diagnostics,
        [
            (DiagnosticKind::PageTreeCycle, offsets[&3]),
            (DiagnosticKind::WrongPageCount, offsets[&2]),
        ]
    );
}

#[test]
fn a_wrong_count_is_reported_and_the_pages_found_are_used() {
    for count in ["/Count 7", "/Count 0", "/Count -1", "/Count (2)", ""] {
        let mut list = vec![
            (1, CATALOG.1.to_string()),
            (2, format!("<< /Type /Pages /Kids [3 0 R 4 0 R] {count} >>")),
        ];
        list.extend([page(3), page(4)]);
        let (file, offsets) = file(&objects(&list));

        let (pages, diagnostics) = pages_of(&file);

        assert_eq!(pages.unwrap(), [Some(3), Some(4)], "{count}");
        assert_eq!(
            diagnostics,
            [(DiagnosticKind::WrongPageCount, offsets[&2])],
            "{count}"
        );
    }
}

#[test]
fn broken_kids_are_skipped() {
    // 9 doesn't exist, 5 isn't a dictionary, 6 is a font and 42 isn't a
    // reference.
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [9 0 R 5 0 R 3 0 R 6 0 R 42] /Count 1 >>".to_string(),
        ),
        (5, "(not a page)".to_string()),
        (6, "<< /Type /Font >>".to_string()),
    ];
    list.push(page(3));
    let (file, offsets) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(3)]);
    let invalid = DiagnosticKind::InvalidPageTreeNode;
    assert_eq!(
        diagnostics,
        [
            (invalid, 0),
            (invalid, offsets[&5]),
            (invalid, offsets[&6]),
            (invalid, offsets[&2]),
        ]
    );
}

#[test]
fn a_kid_that_cannot_be_read_is_kept_as_a_damaged_page() {
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 7 0 R 4 0 R] /Count 3 >>".to_string(),
        ),
        (7, "<< /Type /Page /\x0crocSet >>".to_string()),
    ];
    list.extend([page(3), page(4)]);
    let (file, offsets) = file(&objects(&list));

    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(10);
    let catalog = read_catalog(&mut resolver, &mut diagnostics).unwrap();
    let pages = read_pages(&mut resolver, &catalog, Limits::default(), &mut diagnostics).unwrap();

    let damaged: Vec<_> = pages.iter().map(|p| p.is_damaged()).collect();
    assert_eq!(damaged, [false, true, false]);
    assert_eq!(pages[1].id().map(|id| id.number), Some(7));
    assert!(pages[1].dictionary().is_empty());
    let kinds: Vec<_> = diagnostics.iter().map(|d| (d.kind(), d.offset())).collect();
    assert_eq!(kinds, [(DiagnosticKind::DamagedPage, offsets[&7])]);
}

#[test]
fn a_kids_entry_that_is_not_an_array_gives_no_pages() {
    let list = vec![
        (1, CATALOG.1.to_string()),
        (2, "<< /Type /Pages /Kids 3 0 R /Count 1 >>".to_string()),
        page(3),
    ];
    let (file, offsets) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), []);
    assert_eq!(diagnostics, [(DiagnosticKind::WrongPageCount, offsets[&2])]);
}

#[test]
fn nodes_without_a_type_are_told_apart_by_their_kids() {
    let list = vec![
        (1, CATALOG.1.to_string()),
        (2, "<< /Kids [3 0 R 4 0 R] /Count 2 >>".to_string()),
        (3, "<< /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string()),
        (4, "<< /Kids [5 0 R] /Count 1 >>".to_string()),
        page(5),
    ];
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(3), Some(5)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn a_root_that_is_a_page_is_the_only_page() {
    let list = vec![
        (1, CATALOG.1.to_string()),
        (2, format!("<< /Type /Page {BOX} >>")),
    ];
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(2)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn a_page_held_directly_by_its_parent_has_no_id() {
    let list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            format!("<< /Type /Pages /Kids [<< /Type /Page {BOX} >> 3 0 R] /Count 2 >>"),
        ),
        page(3),
    ];
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [None, Some(3)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn a_page_tree_held_directly_by_the_catalog_is_read() {
    let list = vec![
        (
            1,
            "<< /Type /Catalog /Pages << /Type /Pages /Kids [3 0 R] /Count 1 >> >>".to_string(),
        ),
        page(3),
    ];
    let (file, _) = file(&objects(&list));

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages.unwrap(), [Some(3)]);
    assert!(diagnostics.is_empty());
}

#[test]
fn a_page_tree_root_that_is_not_a_dictionary_is_an_error() {
    for root in ["42", "null", "[3 0 R]"] {
        let list = vec![(1, CATALOG.1.to_string()), (2, root.to_string())];
        let (file, offsets) = file(&objects(&list));

        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        let mut diagnostics = Diagnostics::new(10);
        let catalog = read_catalog(&mut resolver, &mut diagnostics).unwrap();
        let error =
            read_pages(&mut resolver, &catalog, Limits::default(), &mut diagnostics).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidPageTree, "{root}");
        assert_eq!(error.offset(), offsets[&2], "{root}");
    }
}

#[test]
fn too_many_pages_exceed_the_limit() {
    let mut list = vec![
        (1, CATALOG.1.to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
        ),
    ];
    list.extend([page(3), page(4), page(5)]);
    let (file, offsets) = file(&objects(&list));
    let limits = Limits {
        max_pages: 2,
        ..Limits::default()
    };

    let (pages, _) = pages_with(&file, limits);
    assert_eq!(pages.unwrap_err(), ErrorKind::LimitExceeded(Limit::Pages));

    let (pages, _) = pages_with(
        &file,
        Limits {
            max_pages: 3,
            ..limits
        },
    );
    assert_eq!(pages.unwrap().len(), 3);
    assert!(offsets.contains_key(&5));
}

// Real files, with the page counts that ozaru 0.1.7 gives with pdf_oxide.

#[test]
fn the_real_pdfs_have_the_pages_of_the_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let expected = [
        ("crates/ozaru-engine/tests/files/report-flate.pdf", 3),
        ("crates/ozaru-engine/tests/files/report-rc4-128.pdf", 3),
        ("crates/ozaru-engine/tests/files/report-rc4-40.pdf", 3),
        (
            "crates/ozaru-engine/tests/files/report-user-password.pdf",
            3,
        ),
        ("crates/ozaru-engine/tests/files/report-xrefstm.pdf", 3),
        ("docs/assets/damaged-page.pdf", 3),
        ("docs/assets/report.pdf", 3),
        ("tests/factura.pdf", 3),
        ("tests/owner_password.pdf", 1),
        ("tests/page_error.pdf", 3),
        ("tests/page_panic.pdf", 3),
        ("tests/password.pdf", 1),
    ];
    for (name, count) in expected {
        let file = std::fs::read(root.join(name)).unwrap();

        let (pages, diagnostics) = pages_of(&file);

        assert_eq!(pages.map(|pages| pages.len()), Ok(count), "{name}");
        // Its catalog and its third page are damaged on purpose.
        let expected: &[DiagnosticKind] = match name {
            "docs/assets/damaged-page.pdf" => {
                &[DiagnosticKind::PageTreeFound, DiagnosticKind::DamagedPage]
            }
            // One of its pages is damaged on purpose.
            "tests/page_error.pdf" => &[DiagnosticKind::DamagedPage],
            // The /MediaBox key of its third page is damaged on purpose.
            "tests/page_panic.pdf" => &[DiagnosticKind::MissingMediaBox],
            _ => &[],
        };
        let kinds: Vec<_> = diagnostics.iter().map(|&(kind, _)| kind).collect();
        assert_eq!(kinds, expected, "{name}");
    }
}

#[test]
fn the_pdf_whose_page_tree_is_damaged_is_an_error() {
    // Like pdf_oxide, which can't read it either: the header of its page tree,
    // object 8, is damaged into `8 . obj`.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::read(root.join("tests/damaged.pdf")).unwrap();

    let (pages, _) = pages_of(&file);

    assert_eq!(pages.unwrap_err(), ErrorKind::InvalidPageTree);
}
