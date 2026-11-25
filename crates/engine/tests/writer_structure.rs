//! Writing the information dictionary, XMP, bookmarks, page labels,
//! attachments, and cropping, deleting and adding pages.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::{Bookmark, Date, Rectangle, read_xmp};
use ozaru_engine::writer::{Editor, NewAttachment, PageLabel};

mod common;
use common::file_with;

// Four pages, no information dictionary.
fn plain() -> Vec<u8> {
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 200] >>".to_string();
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 >>".to_string(),
        ),
        (3, page.clone()),
        (4, page.clone()),
        (5, page.clone()),
        (6, page),
    ])
}

fn edited(data: &[u8], change: impl FnOnce(&mut Editor<'_, '_>)) -> Vec<u8> {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    change(&mut editor);
    editor.save().unwrap()
}

fn bookmark(title: &str, level: usize, page: Option<usize>, open: bool) -> Bookmark {
    Bookmark {
        title: title.to_string(),
        level,
        page,
        uri: None,
        open,
    }
}

#[test]
fn the_information_dictionary_is_made_and_changed() {
    let saved = edited(&plain(), |editor| {
        editor.set_info_text("Title", Some("Informe")).unwrap();
        editor
            .set_info_text("Author", Some("José Muñoz 日本"))
            .unwrap();
        let date = Date {
            year: 2026,
            month: 11,
            day: 25,
            hour: 10,
            minute: 30,
            second: 5,
            utc_offset: Some(-300),
        };
        editor.set_info_date("CreationDate", Some(&date)).unwrap();
    });
    let document = Document::open(&saved).unwrap();
    let info = document.info().unwrap();
    assert_eq!(info.title().as_deref(), Some("Informe"));
    assert_eq!(info.author().as_deref(), Some("José Muñoz 日本"));
    assert_eq!(
        info.creation_date()
            .map(|d| (d.month, d.day, d.hour, d.utc_offset)),
        Some((11, 25, 10, Some(-300)))
    );
    // Changed and removed.
    let again = edited(&saved, |editor| {
        editor.set_info_text("Title", Some("Otro")).unwrap();
        editor.set_info_text("Author", None).unwrap();
    });
    let document = Document::open(&again).unwrap();
    let info = document.info().unwrap();
    assert_eq!(info.title().as_deref(), Some("Otro"));
    assert_eq!(info.author(), None);
    assert!(info.creation_date().is_some());
}

#[test]
fn removing_from_a_document_without_information_changes_nothing() {
    let saved = edited(&plain(), |editor| {
        editor.set_info_text("Title", None).unwrap();
    });
    assert!(Document::open(&saved).unwrap().info().is_none());
}

#[test]
fn dates_are_written_as_pdf_dates() {
    let date = |offset| Date {
        year: 2024,
        month: 1,
        day: 31,
        hour: 14,
        minute: 30,
        second: 0,
        utc_offset: offset,
    };
    assert_eq!(date(None).to_pdf_string(), "D:20240131143000");
    assert_eq!(date(Some(0)).to_pdf_string(), "D:20240131143000Z");
    assert_eq!(date(Some(60)).to_pdf_string(), "D:20240131143000+01'00'");
    assert_eq!(date(Some(-330)).to_pdf_string(), "D:20240131143000-05'30'");
}

#[test]
fn xmp_is_written_replaced_and_removed() {
    let xml = "<?xpacket begin='' id='W5M0MpCehiHzreSzNTczkc9d'?>\
        <x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF \
        xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
        <rdf:Description xmlns:dc='http://purl.org/dc/elements/1.1/'>\
        <dc:title><rdf:Alt><rdf:li xml:lang='x-default'>Hola</rdf:li></rdf:Alt></dc:title>\
        </rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>";
    let saved = edited(&plain(), |editor| editor.set_xmp(Some(xml)).unwrap());
    let mut document = Document::open(&saved).unwrap();
    let catalog = document.catalog().dictionary().clone();
    let xmp = read_xmp(document.resolver(), &catalog).unwrap();
    assert_eq!(xmp.title().as_deref(), Some("Hola"));
    // Replaced: the same object, not a second one.
    let replaced = edited(&saved, |editor| editor.set_xmp(Some("<x/>")).unwrap());
    let mut document = Document::open(&replaced).unwrap();
    let catalog = document.catalog().dictionary().clone();
    assert!(read_xmp(document.resolver(), &catalog).is_some());
    let removed = edited(&replaced, |editor| editor.set_xmp(None).unwrap());
    let mut document = Document::open(&removed).unwrap();
    let catalog = document.catalog().dictionary().clone();
    assert!(read_xmp(document.resolver(), &catalog).is_none());
}

#[test]
fn bookmarks_are_written_and_read_back() {
    let bookmarks = vec![
        bookmark("Uno", 1, Some(0), true),
        bookmark("Uno.uno ç", 2, Some(1), false),
        bookmark("Uno.uno.uno", 3, Some(2), false),
        bookmark("Uno.dos", 2, None, false),
        bookmark("Dos", 1, Some(3), false),
        Bookmark {
            uri: Some("https://example.com".to_string()),
            ..bookmark("Web", 1, None, false)
        },
    ];
    let saved = edited(&plain(), |editor| editor.set_outlines(&bookmarks).unwrap());
    let mut document = Document::open(&saved).unwrap();
    let read = document.outlines();
    assert_eq!(read, bookmarks);
    // A viewer's counts: "Uno" is open with one visible child, the closed
    // "Uno.uno" hides its own child.
    let catalog = document.catalog().dictionary().clone();
    let outlines = document
        .resolver()
        .resolve(catalog.get("Outlines").unwrap().as_reference().unwrap())
        .unwrap();
    let count = outlines.as_dictionary().unwrap().get("Count").unwrap();
    assert_eq!(count.as_integer(), Some(5));
}

#[test]
fn the_outline_is_replaced_and_removed() {
    let first = edited(&plain(), |editor| {
        editor
            .set_outlines(&[bookmark("A", 1, Some(0), false)])
            .unwrap();
    });
    let second = edited(&first, |editor| {
        editor
            .set_outlines(&[
                bookmark("B", 1, Some(1), false),
                bookmark("C", 1, None, false),
            ])
            .unwrap();
    });
    let titles: Vec<_> = Document::open(&second)
        .unwrap()
        .outlines()
        .into_iter()
        .map(|b| b.title)
        .collect();
    assert_eq!(titles, ["B", "C"]);
    let none = edited(&second, |editor| editor.set_outlines(&[]).unwrap());
    assert!(Document::open(&none).unwrap().outlines().is_empty());
}

#[test]
fn bad_bookmarks_are_refused() {
    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let error = editor
        .set_outlines(&[bookmark("A", 2, None, false)])
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidOutline);
    let error = editor
        .set_outlines(&[bookmark("A", 1, None, false), bookmark("B", 3, None, false)])
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidOutline);
    let error = editor
        .set_outlines(&[bookmark("A", 0, None, false)])
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidOutline);
    let error = editor
        .set_outlines(&[bookmark("A", 1, Some(4), false)])
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PageNotFound);
}

fn label(page: usize, style: Option<char>, prefix: &str, start: i64) -> PageLabel {
    PageLabel {
        page,
        style,
        prefix: prefix.to_string(),
        start,
    }
}

#[test]
fn page_labels_are_written_and_read_back() {
    let saved = edited(&plain(), |editor| {
        editor
            .set_page_labels(&[
                label(0, Some('r'), "", 1),
                label(2, Some('D'), "", 1),
                label(3, Some('A'), "App-", 3),
            ])
            .unwrap();
    });
    let labels = Document::open(&saved).unwrap().page_labels().unwrap();
    assert_eq!(labels, ["i", "ii", "1", "App-C"]);
    let removed = edited(&saved, |editor| editor.set_page_labels(&[]).unwrap());
    assert_eq!(Document::open(&removed).unwrap().page_labels(), None);
}

#[test]
fn bad_page_labels_are_refused() {
    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    for labels in [
        vec![label(1, Some('D'), "", 1)],
        vec![label(0, Some('D'), "", 1), label(0, Some('r'), "", 1)],
        vec![label(0, Some('D'), "", 1), label(4, Some('r'), "", 1)],
        vec![label(0, Some('D'), "", 0)],
        vec![label(0, Some('x'), "", 1)],
    ] {
        let error = editor.set_page_labels(&labels).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidPageLabels, "{labels:?}");
    }
}

fn attachment(name: &str, data: &[u8]) -> NewAttachment {
    NewAttachment {
        name: name.to_string(),
        filename: format!("{name}.txt"),
        data: data.to_vec(),
        description: Some("Una descripción".to_string()),
        mime_type: Some("text/plain".to_string()),
    }
}

fn attachments_of(saved: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut document = Document::open(saved).unwrap();
    document
        .attachments()
        .into_iter()
        .map(|file| {
            let data = document
                .resolver()
                .stream_data(file.stream.unwrap())
                .unwrap()
                .data;
            (file.name, data)
        })
        .collect()
}

#[test]
fn attachments_are_added_replaced_and_removed() {
    let big: Vec<u8> = (0..5000_u32).map(|n| (n % 251) as u8).collect();
    let saved = edited(&plain(), |editor| {
        editor.add_attachment(&attachment("b", b"second")).unwrap();
        editor.add_attachment(&attachment("a", &big)).unwrap();
    });
    assert_eq!(
        attachments_of(&saved),
        [
            ("a".to_string(), big.clone()),
            ("b".to_string(), b"second".to_vec())
        ]
    );
    let document = Document::open(&saved).unwrap();
    let mut document = document;
    let found = document.attachments();
    assert_eq!(found[0].filename.as_deref(), Some("a.txt"));
    assert_eq!(found[0].description.as_deref(), Some("Una descripción"));
    assert_eq!(found[0].mime_type.as_deref(), Some("text/plain"));
    assert_eq!(found[0].size, Some(5000));

    let replaced = edited(&saved, |editor| {
        editor.add_attachment(&attachment("a", b"new")).unwrap();
    });
    assert_eq!(
        attachments_of(&replaced),
        [
            ("a".to_string(), b"new".to_vec()),
            ("b".to_string(), b"second".to_vec())
        ]
    );
    let removed = edited(&replaced, |editor| {
        assert!(editor.remove_attachment("a").unwrap());
        assert!(!editor.remove_attachment("zzz").unwrap());
    });
    assert_eq!(
        attachments_of(&removed),
        [("b".to_string(), b"second".to_vec())]
    );
    let none = edited(&removed, |editor| {
        editor.remove_attachment("b").unwrap();
    });
    assert!(attachments_of(&none).is_empty());
}

#[test]
fn attachments_keep_what_the_file_has() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/navigation.pdf");
    let data = std::fs::read(path).unwrap();
    let saved = edited(&data, |editor| {
        editor.add_attachment(&attachment("middle", b"mm")).unwrap();
    });
    let names: Vec<_> = attachments_of(&saved).into_iter().map(|a| a.0).collect();
    assert_eq!(names, ["data.bin", "middle", "notes.txt"]);
}

#[test]
fn an_attachment_needs_a_name() {
    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let error = editor.add_attachment(&attachment("", b"x")).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidAttachment);
}

#[test]
fn pages_are_cropped() {
    let saved = edited(&plain(), |editor| {
        editor
            .crop_page(1, Rectangle::new(10.0, 20.0, 60.0, 120.0))
            .unwrap();
    });
    let document = Document::open(&saved).unwrap();
    let crop = document.page(1).unwrap().crop_box();
    assert_eq!(
        (crop.left, crop.bottom, crop.right, crop.top),
        (10.0, 20.0, 60.0, 120.0)
    );
    let untouched = document.page(0).unwrap().crop_box();
    assert_eq!((untouched.right, untouched.top), (100.0, 200.0));

    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    for area in [
        Rectangle::new(10.0, 10.0, 10.0, 50.0),
        Rectangle::new(f64::NAN, 0.0, 10.0, 10.0),
    ] {
        assert_eq!(
            editor.crop_page(0, area).unwrap_err().kind(),
            ErrorKind::InvalidBox
        );
    }
    assert_eq!(
        editor.crop_page(9, Rectangle::LETTER).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
}

#[test]
fn pages_are_deleted() {
    let saved = edited(&plain(), |editor| {
        editor
            .crop_page(2, Rectangle::new(0.0, 0.0, 50.0, 50.0))
            .unwrap();
        editor.delete_pages(&[0, 1, 1]).unwrap();
    });
    let document = Document::open(&saved).unwrap();
    assert_eq!(document.page_count(), 2);
    assert_eq!(document.page(0).unwrap().crop_box().right, 50.0);

    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    assert_eq!(
        editor.delete_pages(&[0, 1, 2, 3]).unwrap_err().kind(),
        ErrorKind::NoPagesLeft
    );
    assert_eq!(
        editor.delete_pages(&[4]).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(editor.page_count(), 4);
}

#[test]
fn blank_pages_are_inserted() {
    let saved = edited(&plain(), |editor| {
        editor.insert_blank_page(0, 300.0, 400.0).unwrap();
        editor.insert_blank_page(5, 595.5, 842.0).unwrap();
    });
    let document = Document::open(&saved).unwrap();
    assert_eq!(document.page_count(), 6);
    let size = |index: usize| {
        let page = document.page(index).unwrap().media_box();
        (page.right, page.top)
    };
    assert_eq!(size(0), (300.0, 400.0));
    assert_eq!(size(1), (100.0, 200.0));
    assert_eq!(size(5), (595.5, 842.0));
    assert!(document.diagnostics().is_empty());

    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    assert_eq!(
        editor.insert_blank_page(5, 10.0, 10.0).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    for (width, height) in [(0.0, 10.0), (10.0, -1.0), (f64::INFINITY, 10.0)] {
        assert_eq!(
            editor
                .insert_blank_page(0, width, height)
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidBox
        );
    }
}

#[test]
fn page_labels_follow_pages_only_when_added_at_the_end() {
    let labeled = edited(&plain(), |editor| {
        editor
            .set_page_labels(&[label(0, Some('D'), "", 1)])
            .unwrap();
    });
    let at_end = edited(&labeled, |editor| {
        editor.insert_blank_page(4, 10.0, 10.0).unwrap();
    });
    assert_eq!(
        Document::open(&at_end)
            .unwrap()
            .page_labels()
            .unwrap()
            .len(),
        5
    );
    let before = edited(&labeled, |editor| {
        editor.insert_blank_page(0, 10.0, 10.0).unwrap();
    });
    assert_eq!(Document::open(&before).unwrap().page_labels(), None);
}

#[test]
fn incremental_updates_take_the_new_information() {
    let data = plain();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    editor.set_info_text("Title", Some("Incremental")).unwrap();
    editor
        .set_outlines(&[bookmark("A", 1, Some(0), false)])
        .unwrap();
    let saved = editor.save_incremental().unwrap();
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(
        document.info().and_then(|info| info.title()).as_deref(),
        Some("Incremental")
    );
    assert_eq!(document.outlines().len(), 1);
    assert!(saved.starts_with(&data));
}
