use super::{Object, ObjectId};

/// An object with a number, so others can refer to it (ISO 32000-2, section
/// 7.3.10). In a file it is written `4 0 obj ... endobj`, and `4 0 R` refers to it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndirectObject {
    pub id: ObjectId,
    pub object: Object,
}

impl IndirectObject {
    pub fn new(id: ObjectId, object: Object) -> Self {
        Self { id, object }
    }
}
