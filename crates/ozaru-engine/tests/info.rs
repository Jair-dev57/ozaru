use std::path::Path;

use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::document::{Date, Info, Trapped, decode_text_string, parse_date, read_info};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// Text strings

#[test]
fn ascii_is_the_same_in_pdf_doc_encoding() {
    assert_eq!(
        decode_text_string(b"ozaru sample report"),
        "ozaru sample report"
    );
    assert_eq!(decode_text_string(b""), "");
}

#[test]
fn pdf_doc_encoding_is_latin_1_with_typographic_characters() {
    // Latin-1 from 0xA1: "Año ñandú".
    assert_eq!(decode_text_string(b"A\xF1o \xF1and\xFA"), "Año ñandú");
    assert_eq!(
        decode_text_string(b"\x80\x84\x8D\x8E\x92\x93\xA0"),
        "•—“”™ﬁ€"
    );
    assert_eq!(decode_text_string(b"\x18\x19\x1F"), "˘ˇ˜");
}

#[test]
fn undefined_pdf_doc_codes_are_replacement_characters() {
    assert_eq!(decode_text_string(b"\x7F\x9F"), "\u{FFFD}\u{FFFD}");
    assert_eq!(decode_text_string(b"\xAD"), "\u{AD}");
}

#[test]
fn utf16_big_endian_text_is_decoded() {
    // "Año 𝄞": a character outside the BMP is a surrogate pair.
    let bytes = b"\xFE\xFF\x00A\x00\xF1\x00o\x00 \xD8\x34\xDD\x1E";
    assert_eq!(decode_text_string(bytes), "Año 𝄞");
    assert_eq!(decode_text_string(b"\xFE\xFF"), "");
}

#[test]
fn utf16_little_endian_text_is_decoded_too() {
    assert_eq!(decode_text_string(b"\xFF\xFEA\x00\xF1\x00"), "Añ");
}

#[test]
fn broken_utf16_gets_replacement_characters() {
    // A lone high surrogate, then an odd last byte.
    assert_eq!(
        decode_text_string(b"\xFE\xFF\xD8\x34\x00A\x00"),
        "\u{FFFD}A\u{FFFD}"
    );
}

#[test]
fn language_escapes_are_removed_from_utf16() {
    // ESC "es" ESC before the text.
    let bytes = b"\xFE\xFF\x00\x1B\x00e\x00s\x00\x1B\x00H\x00o\x00l\x00a";
    assert_eq!(decode_text_string(bytes), "Hola");
}

#[test]
fn utf8_text_is_decoded() {
    assert_eq!(decode_text_string("\u{FEFF}Año ✓".as_bytes()), "Año ✓");
    assert_eq!(decode_text_string(b"\xEF\xBB\xBFA\xFF"), "A\u{FFFD}");
}

// Dates

fn date(year: u16, month: u8, day: u8, time: (u8, u8, u8), offset: Option<i16>) -> Date {
    Date {
        year,
        month,
        day,
        hour: time.0,
        minute: time.1,
        second: time.2,
        utc_offset: offset,
    }
}

#[test]
fn a_complete_date_is_parsed() {
    assert_eq!(
        parse_date("D:20240131143005+01'30'"),
        Some(date(2024, 1, 31, (14, 30, 5), Some(90)))
    );
    assert_eq!(
        parse_date("D:20261101124321-05'00'"),
        Some(date(2026, 11, 1, (12, 43, 21), Some(-300)))
    );
    assert_eq!(
        parse_date("D:20000101000000Z"),
        Some(date(2000, 1, 1, (0, 0, 0), Some(0)))
    );
}

#[test]
fn the_parts_after_the_year_are_optional() {
    assert_eq!(
        parse_date("D:2024"),
        Some(date(2024, 1, 1, (0, 0, 0), None))
    );
    assert_eq!(
        parse_date("D:202403"),
        Some(date(2024, 3, 1, (0, 0, 0), None))
    );
    assert_eq!(
        parse_date("D:2024031518"),
        Some(date(2024, 3, 15, (18, 0, 0), None))
    );
    assert_eq!(
        parse_date("D:20240315184500"),
        Some(date(2024, 3, 15, (18, 45, 0), None))
    );
}

#[test]
fn common_variations_are_accepted() {
    let expected = Some(date(2024, 3, 15, (18, 45, 0), Some(60)));
    for text in [
        "D:20240315184500+01'00'",
        "D:20240315184500+01'00",
        "D:20240315184500+0100",
        "D:20240315184500+01",
        "D:20240315184500+01'",
        "20240315184500+01'00'",
        "  D:20240315184500+01'00'  ",
    ] {
        assert_eq!(parse_date(text), expected, "{text}");
    }
    assert_eq!(
        parse_date("D:20240315184500Z00'00'"),
        Some(date(2024, 3, 15, (18, 45, 0), Some(0)))
    );
}

#[test]
fn leap_days_are_checked() {
    assert!(parse_date("D:20240229").is_some());
    assert!(parse_date("D:20000229").is_some());
    assert!(parse_date("D:20230229").is_none());
    assert!(parse_date("D:19000229").is_none());
}

#[test]
fn impossible_or_broken_dates_are_none() {
    for text in [
        "",
        "D:",
        "D:24",
        "D:2024131",
        "D:20241301",
        "D:20240100",
        "D:20240431",
        "D:20240101240000",
        "D:20240101006000",
        "D:20240101000060",
        "D:20240101000000+24'00'",
        "D:20240101000000+01'60'",
        "D:20240101000000Z01'00'",
        "D:20240101000000X",
        "D:20240101 000000",
        "D:2024-01-01",
        "yesterday",
    ] {
        assert_eq!(parse_date(text), None, "{text}");
    }
}

// The information dictionary

// A file with the objects given, and `trailer` in its trailer.
fn file(objects: &[(u32, &str)], trailer: &str) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let table: Vec<(u32, XrefEntry)> = objects
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
    let start = pdf.table(&table, &format!("/Size 20 /Root 1 0 R {trailer}"));
    pdf.end(start);
    pdf.file
}

fn info_of(file: &[u8]) -> (Option<Info>, Vec<DiagnosticKind>) {
    let mut resolver = Resolver::open(file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(10);
    let info = read_info(&mut resolver, &mut diagnostics);
    (info, diagnostics.iter().map(|d| d.kind()).collect())
}

const CATALOG: (u32, &str) = (1, "<< /Type /Catalog /Pages 2 0 R >>");

#[test]
fn every_entry_of_the_information_dictionary_is_read() {
    let file = file(
        &[
            CATALOG,
            (
                3,
                "<< /Title (Informe anual) /Author <FEFF004A006F007300E9> \
                 /Subject (Ventas) /Keywords (pdf, motor) /Creator (Writer) \
                 /Producer (ozaru) /CreationDate (D:20240131143005+01'00') \
                 /ModDate (D:20240201) /Trapped /True /Custom (valor) >>",
            ),
        ],
        "/Info 3 0 R",
    );

    let (info, diagnostics) = info_of(&file);

    let info = info.unwrap();
    assert_eq!(info.title().as_deref(), Some("Informe anual"));
    assert_eq!(info.author().as_deref(), Some("José"));
    assert_eq!(info.subject().as_deref(), Some("Ventas"));
    assert_eq!(info.keywords().as_deref(), Some("pdf, motor"));
    assert_eq!(info.creator().as_deref(), Some("Writer"));
    assert_eq!(info.producer().as_deref(), Some("ozaru"));
    assert_eq!(
        info.creation_date(),
        Some(date(2024, 1, 31, (14, 30, 5), Some(60)))
    );
    assert_eq!(
        info.modification_date(),
        Some(date(2024, 2, 1, (0, 0, 0), None))
    );
    assert_eq!(info.trapped(), Some(Trapped::True));
    assert_eq!(info.text("Custom").as_deref(), Some("valor"));
    assert!(diagnostics.is_empty());
}

#[test]
fn missing_entries_and_entries_that_are_not_strings_are_none() {
    let file = file(
        &[CATALOG, (3, "<< /Title 42 /Trapped /Maybe >>")],
        "/Info 3 0 R",
    );

    let (info, _) = info_of(&file);

    let info = info.unwrap();
    assert_eq!(info.title(), None);
    assert_eq!(info.author(), None);
    assert_eq!(info.creation_date(), None);
    assert_eq!(info.trapped(), None);
}

#[test]
fn a_date_that_cannot_be_parsed_keeps_its_text() {
    let file = file(&[CATALOG, (3, "<< /CreationDate (ayer) >>")], "/Info 3 0 R");

    let (info, _) = info_of(&file);

    let info = info.unwrap();
    assert_eq!(info.creation_date(), None);
    assert_eq!(info.text("CreationDate").as_deref(), Some("ayer"));
}

#[test]
fn trapped_can_be_a_name_or_a_boolean() {
    for (trapped, expected) in [
        ("/True", Trapped::True),
        ("/False", Trapped::False),
        ("/Unknown", Trapped::Unknown),
        ("true", Trapped::True),
        ("false", Trapped::False),
    ] {
        let body = format!("<< /Trapped {trapped} >>");
        let file = file(&[CATALOG, (3, &body)], "/Info 3 0 R");

        let (info, _) = info_of(&file);

        assert_eq!(info.unwrap().trapped(), Some(expected), "{trapped}");
    }
}

#[test]
fn entries_that_are_references_are_resolved() {
    let file = file(
        &[
            CATALOG,
            (3, "<< /Title 4 0 R /Author 9 0 R >>"),
            (4, "(Referenced title)"),
        ],
        "/Info 3 0 R",
    );

    let (info, _) = info_of(&file);

    let info = info.unwrap();
    assert_eq!(info.title().as_deref(), Some("Referenced title"));
    // Object 9 doesn't exist, so the entry is null.
    assert_eq!(info.author(), None);
}

#[test]
fn an_information_dictionary_held_by_the_trailer_is_read() {
    let file = file(&[CATALOG], "/Info << /Title (Direct) >>");

    let (info, diagnostics) = info_of(&file);

    assert_eq!(info.unwrap().title().as_deref(), Some("Direct"));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_file_without_an_information_dictionary_has_none() {
    let (info, diagnostics) = info_of(&file(&[CATALOG], ""));

    assert_eq!(info, None);
    assert!(diagnostics.is_empty());
}

#[test]
fn an_information_entry_to_a_missing_object_is_none() {
    let (info, diagnostics) = info_of(&file(&[CATALOG], "/Info 9 0 R"));

    assert_eq!(info, None);
    assert!(diagnostics.is_empty());
}

#[test]
fn an_information_entry_that_is_not_a_dictionary_is_reported() {
    for (objects, trailer) in [
        (&[CATALOG, (3, "(not info)")][..], "/Info 3 0 R"),
        (&[CATALOG][..], "/Info 42"),
        (&[CATALOG, (3, "<< /Title (x)")][..], "/Info 3 0 R"),
    ] {
        let (info, diagnostics) = info_of(&file(objects, trailer));

        assert_eq!(info, None, "{trailer}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidInfo], "{trailer}");
    }
}

// Real files

fn real_info(name: &str) -> (Option<Info>, Vec<DiagnosticKind>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    info_of(&std::fs::read(root.join(name)).unwrap())
}

#[test]
fn the_sample_report_has_its_information() {
    let (info, diagnostics) = real_info("docs/assets/report.pdf");

    let info = info.unwrap();
    assert_eq!(info.title().as_deref(), Some("ozaru sample report"));
    assert_eq!(info.author().as_deref(), Some("ozaru"));
    assert_eq!(
        info.producer().as_deref(),
        Some("ReportLab PDF Library - (opensource)")
    );
    assert_eq!(
        info.creation_date(),
        Some(date(2000, 1, 1, (0, 0, 0), Some(0)))
    );
    assert_eq!(info.trapped(), Some(Trapped::False));
    assert!(diagnostics.is_empty());
}

#[test]
fn the_encrypted_reports_have_their_information_decrypted() {
    // Ghostscript rewrites of the sample report: their strings are encrypted.
    for name in [
        "crates/ozaru-engine/tests/files/report-rc4-40.pdf",
        "crates/ozaru-engine/tests/files/report-rc4-128.pdf",
    ] {
        let (info, diagnostics) = real_info(name);

        let info = info.unwrap();
        assert_eq!(
            info.title().as_deref(),
            Some("ozaru sample report"),
            "{name}"
        );
        assert_eq!(
            info.producer().as_deref(),
            Some("GPL Ghostscript 10.06.0"),
            "{name}"
        );
        assert!(info.creation_date().is_some(), "{name}");
        assert!(diagnostics.is_empty(), "{name}");
    }
}

#[test]
fn a_locked_file_has_its_information_after_unlocking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let name = "crates/ozaru-engine/tests/files/report-user-password.pdf";
    let file = std::fs::read(root.join(name)).unwrap();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    let mut diagnostics = Diagnostics::new(10);
    assert_eq!(read_info(&mut resolver, &mut diagnostics), None);
    let kinds: Vec<_> = diagnostics.iter().map(|d| d.kind()).collect();
    assert_eq!(kinds, [DiagnosticKind::LockedInfo]);

    assert!(resolver.unlock(b"user"));
    let mut diagnostics = Diagnostics::new(10);
    let info = read_info(&mut resolver, &mut diagnostics).unwrap();
    assert_eq!(info.title().as_deref(), Some("ozaru sample report"));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_damaged_title_key_gives_no_title() {
    // The /Title key of page_panic.pdf is damaged on purpose.
    let (info, _) = real_info("tests/page_panic.pdf");

    let info = info.unwrap();
    assert_eq!(info.title(), None);
    assert_eq!(info.author().as_deref(), Some("Ozaru"));
}
