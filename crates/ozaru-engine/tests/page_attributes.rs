use std::path::Path;

use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::document::{Page, Rectangle, read_catalog, read_pages};
use ozaru_engine::syntax::{Name, Object, ObjectId};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// A file whose objects are all listed in one table, with object 1 as `/Root`.
fn file(objects: &[(u32, &str)]) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut table: Vec<(u32, XrefEntry)> = objects
        .iter()
        .map(|&(number, body)| {
            let offset = pdf.object(number, body);
            let entry = XrefEntry::InUse {
                offset,
                generation: 0,
            };
            (number, entry)
        })
        .collect();
    table.sort_by_key(|&(number, _)| number);
    let start = pdf.table(&table, "/Size 100 /Root 1 0 R");
    pdf.end(start);
    pdf.file
}

fn pages_of(file: &[u8]) -> (Vec<Page>, Vec<DiagnosticKind>) {
    let mut resolver = Resolver::open(file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(100);
    let catalog = read_catalog(&mut resolver, &mut diagnostics).unwrap();
    let pages = read_pages(&mut resolver, &catalog, Limits::default(), &mut diagnostics).unwrap();
    (pages, diagnostics.iter().map(|d| d.kind()).collect())
}

// A file with one page, `page`, under a root with `root` entries.
fn one_page(root: &str, page: &str, extra: &[(u32, &str)]) -> (Page, Vec<DiagnosticKind>) {
    let root = format!("<< /Type /Pages /Kids [3 0 R] /Count 1 {root} >>");
    let page = format!("<< /Type /Page /Parent 2 0 R {page} >>");
    let mut objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        (2, root.as_str()),
        (3, page.as_str()),
    ];
    objects.extend_from_slice(extra);
    let (mut pages, diagnostics) = pages_of(&file(&objects));
    assert_eq!(pages.len(), 1);
    (pages.remove(0), diagnostics)
}

const A4: Rectangle = Rectangle {
    left: 0.0,
    bottom: 0.0,
    right: 595.0,
    top: 842.0,
};

// Inheritance

#[test]
fn a_page_inherits_from_its_nearest_ancestor() {
    // 2 (A4, Rotate 90, resources 5) ── 4 (Rotate 180) ── 3
    //                               └── 6 (its own media box)
    let file = file(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        (
            2,
            "<< /Type /Pages /Kids [4 0 R 6 0 R] /Count 2 /MediaBox [0 0 595 842] /Rotate 90 /Resources 5 0 R >>",
        ),
        (
            4,
            "<< /Type /Pages /Parent 2 0 R /Kids [3 0 R] /Count 1 /Rotate 180 >>",
        ),
        (3, "<< /Type /Page /Parent 4 0 R >>"),
        (6, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>"),
        (5, "<< /Font << /F1 7 0 R >> >>"),
    ]);

    let (pages, diagnostics) = pages_of(&file);

    assert_eq!(pages[0].media_box(), A4);
    assert_eq!(pages[0].rotation(), 180);
    assert_eq!(pages[1].media_box(), Rectangle::LETTER);
    assert_eq!(pages[1].rotation(), 90);
    for page in &pages {
        let fonts = page.resources().category("Font").unwrap();
        assert!(fonts.as_dictionary().unwrap().contains_key("F1"));
    }
    assert!(diagnostics.is_empty());
}

#[test]
fn a_page_attribute_replaces_the_inherited_one() {
    let (page, diagnostics) = one_page(
        "/MediaBox [0 0 595 842] /Rotate 90 /Resources << /Font << /F1 7 0 R >> >>",
        "/MediaBox [0 0 612 792] /Rotate 0 /Resources << /XObject << >> >>",
        &[],
    );

    assert_eq!(page.media_box(), Rectangle::LETTER);
    assert_eq!(page.rotation(), 0);
    assert!(page.resources().category("Font").is_none());
    assert!(page.resources().category("XObject").is_some());
    assert!(diagnostics.is_empty());
}

#[test]
fn the_page_dictionary_only_has_its_own_entries() {
    let (page, _) = one_page("/MediaBox [0 0 595 842]", "/Rotate 90", &[]);

    assert!(!page.dictionary().contains_key("MediaBox"));
    assert!(page.dictionary().contains_key("Rotate"));
}

#[test]
fn attributes_that_are_not_inheritable_are_not_inherited() {
    let (page, _) = one_page(
        "/MediaBox [0 0 595 842] /Contents 9 0 R /Annots []",
        "",
        &[],
    );

    assert!(!page.dictionary().contains_key("Contents"));
    assert!(!page.dictionary().contains_key("Annots"));
}

#[test]
fn a_damaged_page_inherits_from_its_ancestors_without_more_diagnostics() {
    let file = file(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /Rotate 270 /Resources << /Font << >> >> >>",
        ),
        (3, "<< /Type /Page /\x0crocSet >>"),
    ]);

    let (pages, diagnostics) = pages_of(&file);

    assert!(pages[0].is_damaged());
    assert_eq!(pages[0].media_box(), Rectangle::LETTER);
    assert_eq!(pages[0].rotation(), 270);
    assert!(pages[0].resources().category("Font").is_some());
    assert_eq!(diagnostics, [DiagnosticKind::DamagedPage]);
}

// Boxes

#[test]
fn a_box_with_its_corners_swapped_is_put_in_order() {
    let (page, diagnostics) = one_page("", "/MediaBox [612 792 0 0]", &[]);

    assert_eq!(page.media_box(), Rectangle::LETTER);
    assert!(diagnostics.is_empty());
}

#[test]
fn boxes_can_have_real_and_negative_coordinates() {
    let (page, _) = one_page("", "/MediaBox [-10.5 -20 100.25 200]", &[]);

    let media_box = page.media_box();
    assert_eq!(media_box, Rectangle::new(-10.5, -20.0, 100.25, 200.0));
    assert_eq!(media_box.width(), 110.75);
    assert_eq!(media_box.height(), 220.0);
}

#[test]
fn a_missing_media_box_is_us_letter() {
    let (page, diagnostics) = one_page("", "", &[]);

    assert_eq!(page.media_box(), Rectangle::LETTER);
    assert_eq!(page.crop_box(), Rectangle::LETTER);
    assert_eq!(diagnostics, [DiagnosticKind::MissingMediaBox]);
}

#[test]
fn an_invalid_media_box_is_us_letter() {
    for media_box in [
        "[0 0 612]",
        "[0 0 612 792 5]",
        "[0 0 0 0]",
        "[0 0 612 0]",
        "[(a) 0 612 792]",
        "5",
        "null",
        "9 0 R",
    ] {
        let (page, diagnostics) = one_page("", &format!("/MediaBox {media_box}"), &[]);

        assert_eq!(page.media_box(), Rectangle::LETTER, "{media_box}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidPageBox], "{media_box}");
    }
}

#[test]
fn without_a_crop_box_it_is_the_media_box() {
    let (page, diagnostics) = one_page("/MediaBox [0 0 595 842]", "", &[]);

    assert_eq!(page.crop_box(), A4);
    assert!(diagnostics.is_empty());
}

#[test]
fn the_crop_box_is_clipped_to_the_media_box() {
    let (page, diagnostics) = one_page(
        "/MediaBox [0 0 595 842]",
        "/CropBox [-50 100 300 1000]",
        &[],
    );

    assert_eq!(page.crop_box(), Rectangle::new(0.0, 100.0, 300.0, 842.0));
    assert!(diagnostics.is_empty());
}

#[test]
fn an_unusable_crop_box_is_the_media_box() {
    for crop_box in ["[600 0 700 100]", "[0 0 0 0]", "[0 0 1]", "(crop)"] {
        let (page, diagnostics) = one_page(
            "/MediaBox [0 0 595 842]",
            &format!("/CropBox {crop_box}"),
            &[],
        );

        assert_eq!(page.crop_box(), A4, "{crop_box}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidPageBox], "{crop_box}");
    }
}

#[test]
fn the_crop_box_is_inherited_too() {
    let (page, _) = one_page("/MediaBox [0 0 595 842] /CropBox [10 10 500 800]", "", &[]);

    assert_eq!(page.crop_box(), Rectangle::new(10.0, 10.0, 500.0, 800.0));
}

// Rotation

#[test]
fn rotations_are_brought_into_0_to_270() {
    for (rotate, expected) in [
        ("0", 0),
        ("90", 90),
        ("180", 180),
        ("270", 270),
        ("360", 0),
        ("450", 90),
        ("-90", 270),
        ("-180", 180),
        ("-450", 270),
        ("90.0", 90),
    ] {
        let (page, diagnostics) =
            one_page("/MediaBox [0 0 595 842]", &format!("/Rotate {rotate}"), &[]);

        assert_eq!(page.rotation(), expected, "{rotate}");
        assert!(diagnostics.is_empty(), "{rotate}");
    }
}

#[test]
fn an_invalid_rotation_is_none() {
    for rotate in ["45", "90.5", "(90)", "/R90", "null"] {
        let (page, diagnostics) =
            one_page("/MediaBox [0 0 595 842]", &format!("/Rotate {rotate}"), &[]);

        assert_eq!(page.rotation(), 0, "{rotate}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidRotation], "{rotate}");
    }
}

// Resources

#[test]
fn resources_are_kept_as_they_are_in_the_file() {
    let (page, _) = one_page(
        "/MediaBox [0 0 595 842]",
        "/Resources << /Font 5 0 R /ProcSet [/PDF /Text] >>",
        &[(5, "<< /F1 6 0 R >>")],
    );

    let resources = page.resources();
    assert_eq!(
        resources.category("Font"),
        Some(&Object::Reference(ObjectId::new(5, 0)))
    );
    assert_eq!(
        resources
            .dictionary()
            .get("ProcSet")
            .and_then(Object::as_array),
        Some(
            &[
                Object::Name(Name::from("PDF")),
                Object::Name(Name::from("Text"))
            ][..]
        )
    );
}

#[test]
fn a_page_without_resources_has_none() {
    let (page, diagnostics) = one_page("/MediaBox [0 0 595 842]", "", &[]);

    assert!(page.resources().is_empty());
    assert!(diagnostics.is_empty());
}

#[test]
fn resources_that_are_not_a_dictionary_are_none() {
    for resources in ["5", "[/Font]", "9 0 R", "8 0 R"] {
        let (page, diagnostics) = one_page(
            "/MediaBox [0 0 595 842]",
            &format!("/Resources {resources}"),
            &[(8, "(not resources)")],
        );

        assert!(page.resources().is_empty(), "{resources}");
        assert_eq!(
            diagnostics,
            [DiagnosticKind::InvalidResources],
            "{resources}"
        );
    }
}

// References

#[test]
fn attributes_can_be_references() {
    let (page, diagnostics) = one_page(
        "/MediaBox 5 0 R /Resources 7 0 R",
        "/Rotate 6 0 R /CropBox [0 0 8 0 R 400]",
        &[
            (5, "[0 0 595 842]"),
            (6, "-90"),
            (7, "<< /Font << >> >>"),
            (8, "300"),
        ],
    );

    assert_eq!(page.media_box(), A4);
    assert_eq!(page.crop_box(), Rectangle::new(0.0, 0.0, 300.0, 400.0));
    assert_eq!(page.rotation(), 270);
    assert!(page.resources().category("Font").is_some());
    assert!(diagnostics.is_empty());
}

// Real files

#[test]
fn the_pages_of_the_real_pdfs_are_a4_with_fonts() {
    // Every real PDF of the repository is an A4 sample written by ReportLab, or
    // a rewrite of one, with its fonts in the resources of each page.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = [
        "crates/ozaru-engine/tests/files/report-flate.pdf",
        "crates/ozaru-engine/tests/files/report-rc4-128.pdf",
        "crates/ozaru-engine/tests/files/report-rc4-40.pdf",
        "crates/ozaru-engine/tests/files/report-user-password.pdf",
        "crates/ozaru-engine/tests/files/report-xrefstm.pdf",
        "docs/assets/report.pdf",
        "tests/factura.pdf",
        "tests/owner_password.pdf",
        "tests/page_error.pdf",
        "tests/password.pdf",
    ];
    for name in names {
        let file = std::fs::read(root.join(name)).unwrap();

        let (pages, _) = pages_of(&file);

        for (number, page) in pages.iter().enumerate() {
            let label = format!("{name}, page {}", number + 1);
            if page.is_damaged() {
                continue;
            }
            assert!(
                (page.media_box().width() - 595.2756).abs() < 0.01,
                "{label}"
            );
            assert!(
                (page.media_box().height() - 841.8898).abs() < 0.01,
                "{label}"
            );
            assert_eq!(page.crop_box(), page.media_box(), "{label}");
            assert_eq!(page.rotation(), 0, "{label}");
            assert!(page.resources().category("Font").is_some(), "{label}");
        }
    }
}
