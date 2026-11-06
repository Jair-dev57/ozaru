use std::path::{Path, PathBuf};

use ozaru_engine::base::{DiagnosticKind, Diagnostics, ErrorKind, Limits};
use ozaru_engine::document::{Catalog, read_catalog};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

fn id(number: u32) -> ObjectId {
    ObjectId::new(number, 0)
}

fn in_use(offset: usize) -> XrefEntry {
    XrefEntry::InUse {
        offset,
        generation: 0,
    }
}

// A file whose objects are all listed in one table, with `trailer`. It returns
// the file and the offset of each object.
fn file(objects: &[(u32, &str)], trailer: &str) -> (Vec<u8>, Vec<usize>) {
    let mut pdf = Pdf::new();
    let offsets: Vec<usize> = objects
        .iter()
        .map(|&(number, body)| pdf.object(number, body))
        .collect();
    let table: Vec<(u32, XrefEntry)> = objects
        .iter()
        .zip(&offsets)
        .map(|(&(number, _), &offset)| (number, in_use(offset)))
        .collect();
    let start = pdf.table(&table, trailer);
    pdf.end(start);
    (pdf.file, offsets)
}

fn catalog_of(file: &[u8]) -> (Result<Catalog, ErrorKind>, Vec<(DiagnosticKind, usize)>) {
    let mut resolver = Resolver::open(file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(100);
    let catalog = read_catalog(&mut resolver, &mut diagnostics).map_err(|e| e.kind());
    let diagnostics = diagnostics.iter().map(|d| (d.kind(), d.offset())).collect();
    (catalog, diagnostics)
}

const PAGES: (u32, &str) = (2, "<< /Type /Pages /Kids [] /Count 0 >>");
const FONT: (u32, &str) = (2, "<< /Type /Font /Subtype /Type1 >>");

#[test]
fn the_root_of_the_trailer_is_the_catalog() {
    let (file, _) = file(
        &[(1, "<< /Type /Catalog /Pages 2 0 R >>"), PAGES],
        "/Size 3 /Root 1 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), Some(id(1)));
    assert_eq!(catalog.pages(), &Object::Reference(id(2)));
    assert!(catalog.dictionary().contains_key("Type"));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_catalog_without_its_type_is_used_with_a_diagnostic() {
    let (file, offsets) = file(&[(1, "<< /Pages 2 0 R >>"), PAGES], "/Size 3 /Root 1 0 R");

    let (catalog, diagnostics) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
    assert_eq!(
        diagnostics,
        [(DiagnosticKind::MissingCatalogType, offsets[0])]
    );
}

#[test]
fn a_catalog_with_another_type_is_used_with_a_diagnostic() {
    let (file, offsets) = file(
        &[(1, "<< /Type /Pages /Pages 2 0 R >>"), PAGES],
        "/Size 3 /Root 1 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
    assert_eq!(
        diagnostics,
        [(DiagnosticKind::MissingCatalogType, offsets[0])]
    );
}

#[test]
fn a_catalog_held_directly_by_the_trailer_has_no_id() {
    let (file, _) = file(&[PAGES], "/Size 3 /Root << /Type /Catalog /Pages 2 0 R >>");

    let (catalog, diagnostics) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), None);
    assert_eq!(catalog.pages(), &Object::Reference(id(2)));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_page_tree_held_directly_by_the_catalog_is_accepted() {
    let (file, _) = file(
        &[(
            1,
            "<< /Type /Catalog /Pages << /Type /Pages /Kids [] /Count 0 >> >>",
        )],
        "/Size 2 /Root 1 0 R",
    );

    let (catalog, _) = catalog_of(&file);

    assert!(catalog.unwrap().pages().as_dictionary().is_some());
}

// When the root can't be used, another catalog is searched for.

#[test]
fn without_a_root_the_catalog_is_found_among_the_objects() {
    let (file, offsets) = file(
        &[(1, "<< /Type /Catalog /Pages 2 0 R >>"), PAGES],
        "/Size 3",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
    assert_eq!(diagnostics, [(DiagnosticKind::CatalogFound, offsets[0])]);
}

#[test]
fn a_root_that_is_not_a_dictionary_is_replaced_by_a_catalog() {
    let (file, offsets) = file(
        &[(1, "42"), PAGES, (3, "<< /Type /Catalog /Pages 2 0 R >>")],
        "/Size 4 /Root 1 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(3)));
    assert_eq!(diagnostics, [(DiagnosticKind::CatalogFound, offsets[2])]);
}

#[test]
fn a_root_to_a_missing_object_is_replaced_by_a_catalog() {
    let (file, offsets) = file(
        &[PAGES, (3, "<< /Type /Catalog /Pages 2 0 R >>")],
        "/Size 10 /Root 9 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(3)));
    assert_eq!(diagnostics, [(DiagnosticKind::CatalogFound, offsets[1])]);
}

#[test]
fn a_root_without_pages_is_replaced_by_a_catalog() {
    let (file, _) = file(
        &[
            (1, "<< /Type /Catalog /Pages 5 >>"),
            PAGES,
            (3, "<< /Type /Catalog /Pages 2 0 R >>"),
        ],
        "/Size 4 /Root 1 0 R",
    );

    let (catalog, _) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(3)));
}

#[test]
fn the_search_ignores_dictionaries_that_are_not_catalogs() {
    let (file, _) = file(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>"),
            PAGES,
            (3, "<< /Pages 2 0 R >>"),
            (4, "<< /Type /Catalog >>"),
        ],
        "/Size 5",
    );

    let (catalog, _) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
}

#[test]
fn the_catalog_furthest_into_the_file_wins() {
    // Object 5 is written before object 1, so object 1 is the newer one.
    let (file, _) = file(
        &[
            (5, "<< /Type /Catalog /Pages 2 0 R /Version /Old >>"),
            PAGES,
            (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        ],
        "/Size 6",
    );

    let (catalog, _) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
}

#[test]
fn a_catalog_inside_an_object_stream_is_found() {
    let mut pdf = Pdf::new();
    let two = pdf.object(2, PAGES.1);
    let ten = pdf.object_stream(10, &[(1, "<< /Type /Catalog /Pages 2 0 R >>")]);
    let start = pdf.xref_stream(
        11,
        &[
            (
                1,
                XrefEntry::Compressed {
                    stream: 10,
                    index: 0,
                },
            ),
            (2, in_use(two)),
            (10, in_use(ten)),
        ],
        "/Size 12",
    );
    pdf.end(start);

    let (catalog, diagnostics) = catalog_of(&pdf.file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
    assert_eq!(diagnostics, [(DiagnosticKind::CatalogFound, ten)]);
}

// When no catalog has usable /Pages, the root of the page tree is searched for.

#[test]
fn a_catalog_with_a_pages_entry_that_is_not_a_reference_gets_the_page_tree_root() {
    let (file, offsets) = file(
        &[
            (1, "<< /Type /Catalog /PageMode /UseNone /Pages 9 >>"),
            (3, "<< /Type /Pages /Parent 9 0 R /Kids [] /Count 0 >>"),
            (9, "<< /Type /Pages /Kids [3 0 R] /Count 0 >>"),
        ],
        "/Size 10 /Root 1 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), Some(id(1)));
    assert_eq!(catalog.pages(), &Object::Reference(id(9)));
    assert!(catalog.dictionary().contains_key("PageMode"));
    assert_eq!(diagnostics, [(DiagnosticKind::PageTreeFound, offsets[2])]);
}

#[test]
fn an_unreadable_catalog_is_replaced_by_one_for_the_page_tree_root() {
    // The damage of docs/assets/damaged-page.pdf: `9 09R` instead of `9 0 R`
    // makes the whole dictionary unreadable.
    let (file, offsets) = file(
        &[
            (1, "<< /Type /Catalog /Pages 9 09R >>"),
            (9, "<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        "/Size 10 /Root 1 0 R",
    );

    let (catalog, diagnostics) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), None);
    assert_eq!(catalog.pages(), &Object::Reference(id(9)));
    assert_eq!(diagnostics, [(DiagnosticKind::PageTreeFound, offsets[1])]);
}

#[test]
fn a_broken_catalog_found_by_searching_gets_the_page_tree_root() {
    let (file, _) = file(&[(1, "<< /Type /Catalog /Lang (es) >>"), PAGES], "/Size 3");

    let (catalog, _) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), Some(id(1)));
    assert_eq!(catalog.pages(), &Object::Reference(id(2)));
    assert!(catalog.dictionary().contains_key("Lang"));
}

#[test]
fn without_any_catalog_one_is_made_for_the_page_tree_root() {
    let (file, offsets) = file(&[PAGES], "/Size 3");

    let (catalog, diagnostics) = catalog_of(&file);

    let catalog = catalog.unwrap();
    assert_eq!(catalog.id(), None);
    assert_eq!(catalog.pages(), &Object::Reference(id(2)));
    assert!(catalog.dictionary().contains_key("Type"));
    assert_eq!(diagnostics, [(DiagnosticKind::PageTreeFound, offsets[0])]);
}

#[test]
fn a_page_tree_node_with_a_parent_is_not_taken_for_the_root() {
    let (file, _) = file(
        &[(1, "<< /Type /Pages /Parent 5 0 R /Kids [] /Count 0 >>")],
        "/Size 2",
    );

    let (catalog, _) = catalog_of(&file);

    assert_eq!(catalog.unwrap_err(), ErrorKind::MissingCatalog);
}

#[test]
fn a_file_without_any_catalog_is_an_error() {
    let (file, offsets) = file(&[(1, "42"), FONT], "/Size 3 /Root 1 0 R");

    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let error = read_catalog(&mut resolver, &mut Diagnostics::new(10)).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingCatalog);
    assert_eq!(error.offset(), offsets[0]);
}

#[test]
fn a_file_without_a_root_or_catalog_is_an_error_at_the_start() {
    let (file, _) = file(&[FONT], "/Size 3");

    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let error = read_catalog(&mut resolver, &mut Diagnostics::new(10)).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingCatalog);
    assert_eq!(error.offset(), 0);
}

#[test]
fn a_catalog_that_refers_to_itself_is_not_a_problem() {
    let (file, _) = file(
        &[(1, "<< /Type /Catalog /Pages 2 0 R /Self 1 0 R >>"), PAGES],
        "/Size 3 /Root 1 0 R",
    );

    let (catalog, _) = catalog_of(&file);

    assert_eq!(catalog.unwrap().id(), Some(id(1)));
}

// Real files

fn pdf_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<PathBuf> = ["tests", "docs/assets", "crates/ozaru-engine/tests/files"]
        .iter()
        .flat_map(|folder| std::fs::read_dir(root.join(folder)).expect("folder should exist"))
        .map(|entry| entry.expect("entry should be readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pdf"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_real_pdf_has_a_catalog() {
    let files = pdf_files();
    assert!(files.len() >= 10);
    for path in files {
        let file = std::fs::read(&path).unwrap();
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        let mut diagnostics = Diagnostics::new(100);

        let catalog = read_catalog(&mut resolver, &mut diagnostics);

        let catalog = catalog.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let kinds: Vec<_> = diagnostics.iter().map(|d| d.kind()).collect();
        // The sample of damaged pages also has its catalog damaged.
        let expected: &[DiagnosticKind] = if path.ends_with("docs/assets/damaged-page.pdf") {
            &[DiagnosticKind::PageTreeFound]
        } else {
            &[]
        };
        assert_eq!(kinds, expected, "{}", path.display());
        assert_eq!(
            catalog.id().is_some(),
            expected.is_empty(),
            "{}",
            path.display()
        );
    }
}
