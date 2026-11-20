use ozaru_engine::Document;
use ozaru_engine::document::{Date, Xmp, parse_xmp_date, read_xmp};

mod common;
use common::{file_with, stream_object};

const PACKET: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
   <dc:title><rdf:Alt>
     <rdf:li xml:lang="es">Informe anual</rdf:li>
     <rdf:li xml:lang="x-default">Annual report &amp; outlook</rdf:li>
   </rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>José Núñez</rdf:li><rdf:li>Ana Ruiz</rdf:li></rdf:Seq></dc:creator>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">Sales</rdf:li></rdf:Alt></dc:description>
   <dc:subject><rdf:Bag><rdf:li>sales</rdf:li><rdf:li>2026</rdf:li></rdf:Bag></dc:subject>
   <xmp:CreatorTool>Writer</xmp:CreatorTool>
   <xmp:CreateDate>2026-03-04T10:20:30-05:00</xmp:CreateDate>
   <xmp:ModifyDate>2026-03-05T08:00Z</xmp:ModifyDate>
   <pdf:Producer><![CDATA[ozaru <test>]]></pdf:Producer>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

#[test]
fn a_packet_gives_the_properties_of_the_information_dictionary() {
    let xmp = Xmp::parse(PACKET).unwrap();
    assert_eq!(xmp.title().as_deref(), Some("Annual report & outlook"));
    assert_eq!(xmp.author().as_deref(), Some("José Núñez; Ana Ruiz"));
    assert_eq!(xmp.subject().as_deref(), Some("Sales"));
    // Without pdf:Keywords, the subjects of dc:subject.
    assert_eq!(xmp.keywords().as_deref(), Some("sales, 2026"));
    assert_eq!(xmp.creator().as_deref(), Some("Writer"));
    assert_eq!(xmp.producer().as_deref(), Some("ozaru <test>"));
    let created = xmp.creation_date().unwrap();
    assert_eq!(
        (
            created.year,
            created.month,
            created.day,
            created.hour,
            created.second
        ),
        (2026, 3, 4, 10, 30)
    );
    assert_eq!(created.utc_offset, Some(-300));
    assert_eq!(xmp.modification_date().unwrap().utc_offset, Some(0));
    assert_eq!(xmp.xml(), PACKET);
}

#[test]
fn properties_can_be_attributes_with_any_prefix() {
    let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <r:Description xmlns:p="http://ns.adobe.com/pdf/1.3/" xmlns:t="http://ns.adobe.com/xap/1.0/"
        p:Producer="Ghostscript &#x2014; 10" p:Keywords='a, b' t:CreatorTool="Tool"/>
      </r:RDF></x:xmpmeta>"#;
    let xmp = Xmp::parse(packet).unwrap();
    assert_eq!(xmp.producer().as_deref(), Some("Ghostscript — 10"));
    assert_eq!(xmp.keywords().as_deref(), Some("a, b"));
    assert_eq!(xmp.creator().as_deref(), Some("Tool"));
    assert_eq!(xmp.title(), None);
}

#[test]
fn a_prefix_of_another_namespace_isnt_taken() {
    let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:dc="http://example.com/other/"><dc:title>Not this</dc:title></rdf:Description>
      </rdf:RDF>"#;
    assert_eq!(Xmp::parse(packet).unwrap().title(), None);
}

#[test]
fn xmp_dates_are_iso_8601() {
    let date = |text: &str| {
        parse_xmp_date(text).map(|d: Date| (d.year, d.month, d.day, d.hour, d.minute, d.utc_offset))
    };
    assert_eq!(date("2026"), Some((2026, 1, 1, 0, 0, None)));
    assert_eq!(date("2026-07"), Some((2026, 7, 1, 0, 0, None)));
    assert_eq!(date("2026-07-09T14:05"), Some((2026, 7, 9, 14, 5, None)));
    assert_eq!(
        date("2026-07-09T14:05:30.25+02:30"),
        Some((2026, 7, 9, 14, 5, Some(150)))
    );
    for wrong in [
        "",
        "26-13",
        "2026-13-01",
        "2026-07-09T25:00",
        "today",
        "2026-07-09-01",
    ] {
        assert_eq!(date(wrong), None, "{wrong}");
    }
}

#[test]
fn damaged_xml_reads_what_it_can() {
    // An end tag that doesn't match, and an element left open.
    let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description
      xmlns:pdf="http://ns.adobe.com/pdf/1.3/"><pdf:Producer>Odd</pdf:Other><pdf:Keywords>k"#;
    let xmp = Xmp::parse(packet).unwrap();
    assert_eq!(xmp.producer().as_deref(), Some("Odd"));
    // A tag that never closes is no XML.
    assert!(Xmp::parse("<rdf:RDF").is_none());
}

#[test]
fn the_packet_is_read_from_the_catalog() {
    let pdf = file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Metadata 4 0 R >>".to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        ),
        (4, stream_object("/Type /Metadata /Subtype /XML", PACKET)),
    ]);
    let mut document = Document::open(&pdf).unwrap();
    let catalog = document.catalog().dictionary().clone();
    let xmp = read_xmp(document.resolver(), &catalog).unwrap();
    assert_eq!(xmp.title().as_deref(), Some("Annual report & outlook"));
    // Without /Metadata, there is none.
    let pdf = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        ),
    ]);
    let mut document = Document::open(&pdf).unwrap();
    let catalog = document.catalog().dictionary().clone();
    assert!(read_xmp(document.resolver(), &catalog).is_none());
}
