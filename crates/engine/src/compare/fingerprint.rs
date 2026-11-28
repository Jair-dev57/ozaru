//! A fingerprint of what a page draws, from its content, resources and
//! annotations, with the numbers of its objects left out: the same page in two
//! files has the same fingerprint, and pages with different ones may still
//! look the same.

use std::collections::HashMap;

use crate::document::Page;
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

// FNV-1a over bytes, mixed in as they come.
#[derive(Clone, Copy)]
struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, data: &[u8]) {
        for &byte in data {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn tag(&mut self, tag: u8) {
        self.bytes(&[tag]);
    }

    fn number(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
}

// The entries of a page that don't change what it looks like, or that point
// back into the tree.
const SKIPPED: [&str; 8] = [
    "Parent",
    "B",
    "Thumb",
    "Metadata",
    "PieceInfo",
    "LastModified",
    "StructParents",
    "AA",
];

// The deepest and the most data walked.
const MAX_DEPTH: usize = 48;
const MAX_BYTES: usize = 1 << 29;

struct Walk<'a, 'd> {
    resolver: &'a mut Resolver<'d>,
    memo: HashMap<ObjectId, u64>,
    bytes: usize,
    // Whether a limit was reached: the fingerprint can't be trusted.
    truncated: bool,
}

impl Walk<'_, '_> {
    fn object(&mut self, object: &Object, depth: usize) -> u64 {
        let mut hasher = Hasher::new();
        if depth > MAX_DEPTH {
            self.truncated = true;
            return 0;
        }
        match object {
            Object::Null => hasher.tag(0),
            Object::Boolean(value) => {
                hasher.tag(1);
                hasher.tag(u8::from(*value));
            }
            Object::Integer(value) => {
                hasher.tag(2);
                hasher.number(*value as u64);
            }
            // The same number written with other digits is the same.
            Object::Real(value) => {
                hasher.tag(2);
                if value.fract() == 0.0 && value.abs() < 1e15 {
                    hasher.number(*value as i64 as u64);
                } else {
                    hasher.tag(3);
                    hasher.number(value.to_bits());
                }
            }
            Object::String(string) => {
                hasher.tag(4);
                hasher.bytes(string.as_bytes());
            }
            Object::Name(name) => {
                hasher.tag(5);
                hasher.bytes(name.as_bytes());
            }
            Object::Array(items) => {
                hasher.tag(6);
                for item in items {
                    hasher.number(self.object(item, depth + 1));
                }
            }
            Object::Dictionary(dictionary) => {
                hasher.tag(7);
                hasher.number(self.dictionary(dictionary, depth + 1));
            }
            Object::Stream(stream) => {
                hasher.tag(8);
                hasher.number(self.dictionary(stream.dictionary(), depth + 1));
            }
            Object::Reference(id) => {
                hasher.tag(9);
                hasher.number(self.reference(*id, depth + 1));
            }
        }
        hasher.0
    }

    // A dictionary's entries in the order of their keys.
    fn dictionary(&mut self, dictionary: &Dictionary, depth: usize) -> u64 {
        let mut entries: Vec<(&[u8], &Object)> = dictionary
            .iter()
            .filter(|(key, _)| {
                !SKIPPED
                    .iter()
                    .any(|skipped| skipped.as_bytes() == key.as_bytes())
            })
            .map(|(key, value)| (key.as_bytes(), value))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut hasher = Hasher::new();
        for (key, value) in entries {
            hasher.bytes(key);
            hasher.number(self.object(value, depth));
        }
        hasher.0
    }

    // What an object holds, once for each object.
    fn reference(&mut self, id: ObjectId, depth: usize) -> u64 {
        if let Some(&known) = self.memo.get(&id) {
            return known;
        }
        // A cycle meets itself here, as a fixed value.
        self.memo.insert(id, 0xdead);
        let Ok(object) = self.resolver.resolve(id) else {
            return 0;
        };
        let mut hasher = Hasher::new();
        hasher.number(self.object(&object, depth));
        if object.as_stream().is_some() {
            match self.resolver.decrypted_stream(id) {
                Ok((_, data)) => {
                    self.bytes += data.len();
                    if self.bytes > MAX_BYTES {
                        self.truncated = true;
                    }
                    hasher.bytes(&data);
                }
                Err(_) => hasher.tag(255),
            }
        }
        self.memo.insert(id, hasher.0);
        hasher.0
    }
}

/// A number that is the same for pages that draw the same, in any file. `None`
/// if the page is too large to look through, or can't be read.
pub fn page_fingerprint(resolver: &mut Resolver<'_>, page: &Page) -> Option<u64> {
    if page.is_damaged() {
        return None;
    }
    let mut walk = Walk {
        resolver,
        memo: HashMap::new(),
        bytes: 0,
        truncated: false,
    };
    let mut hasher = Hasher::new();
    hasher.number(walk.dictionary(page.dictionary(), 0));
    // What the page inherits from the page tree.
    for key in ["Resources", "MediaBox", "CropBox", "Rotate"] {
        if !page.dictionary().contains_key(key)
            && let Some(inherited) = page.attribute(key)
        {
            hasher.bytes(key.as_bytes());
            hasher.number(walk.object(inherited, 1));
        }
    }
    (!walk.truncated).then_some(hasher.0)
}
