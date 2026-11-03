//! Helpers shared by the integration tests.

#![allow(dead_code)]

use ozaru_engine::xref::XrefEntry;

/// Writes PDF files by hand, keeping track of offsets.
pub struct Pdf {
    pub file: Vec<u8>,
}

impl Pdf {
    pub fn new() -> Self {
        Self {
            file: b"%PDF-1.7\n".to_vec(),
        }
    }

    pub fn offset(&self) -> usize {
        self.file.len()
    }

    pub fn object(&mut self, number: u32, body: &str) -> usize {
        let offset = self.offset();
        self.write(&format!("{number} 0 obj\n{body}\nendobj\n"));
        offset
    }

    // A classic table with one subsection per entry, and its trailer.
    pub fn table(&mut self, entries: &[(u32, XrefEntry)], trailer: &str) -> usize {
        let offset = self.offset();
        self.write("xref\n");
        for &(number, entry) in entries {
            let (value, generation, kind) = match entry {
                XrefEntry::InUse { offset, generation } => (offset, generation, 'n'),
                XrefEntry::Free { generation } => (0, generation, 'f'),
                XrefEntry::Compressed { .. } => panic!("tables can't hold compressed entries"),
            };
            self.write(&format!(
                "{number} 1\n{value:010} {generation:05} {kind} \n"
            ));
        }
        self.write(&format!("trailer\n<< {trailer} >>\n"));
        offset
    }

    // A cross-reference stream object `number`, uncompressed, with /W [1 4 2].
    pub fn xref_stream(
        &mut self,
        number: u32,
        entries: &[(u32, XrefEntry)],
        trailer: &str,
    ) -> usize {
        let mut index = String::new();
        let mut data = Vec::new();
        for &(entry_number, entry) in entries {
            index.push_str(&format!("{entry_number} 1 "));
            let (kind, second, third) = match entry {
                XrefEntry::Free { generation } => (0, 0, u32::from(generation)),
                XrefEntry::InUse { offset, generation } => {
                    (1, offset as u32, u32::from(generation))
                }
                XrefEntry::Compressed { stream, index } => (2, stream, index),
            };
            data.push(kind);
            data.extend(second.to_be_bytes());
            data.extend((third as u16).to_be_bytes());
        }
        let offset = self.offset();
        self.write(&format!(
            "{number} 0 obj\n<< /Type /XRef /W [1 4 2] /Index [{index}] {trailer} /Length {} >>\nstream\n",
            data.len()
        ));
        self.file.extend(data);
        self.write("\nendstream\nendobj\n");
        offset
    }

    pub fn end(&mut self, startxref: usize) {
        self.write(&format!("startxref\n{startxref}\n%%EOF\n"));
    }

    // An object stream `number` holding `objects`, compressed with Flate.
    pub fn object_stream(&mut self, number: u32, objects: &[(u32, &str)]) -> usize {
        let mut header = String::new();
        let mut body = String::new();
        for (object, text) in objects {
            header.push_str(&format!("{object} {} ", body.len()));
            body.push_str(text);
            body.push(' ');
        }
        let data =
            miniz_oxide::deflate::compress_to_vec_zlib(format!("{header}{body}").as_bytes(), 6);
        let offset = self.offset();
        self.write(&format!(
            "{number} 0 obj\n<< /Type /ObjStm /N {} /First {} /Filter /FlateDecode /Length {} >>\nstream\n",
            objects.len(),
            header.len(),
            data.len()
        ));
        self.file.extend(data);
        self.write("\nendstream\nendobj\n");
        offset
    }

    pub fn write(&mut self, text: &str) {
        self.file.extend_from_slice(text.as_bytes());
    }
}
