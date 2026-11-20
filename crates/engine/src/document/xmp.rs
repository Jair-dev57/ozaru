use super::Date;
use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// The XMP metadata of a document: the XML of the catalog's `/Metadata`
/// stream (ISO 32000-2, section 14.3.2), with the properties that the
/// information dictionary also has.
#[derive(Debug, Clone, PartialEq)]
pub struct Xmp {
    xml: String,
    root: Node,
}

// Namespaces of the properties read, by their URIs: their prefixes are the
// ones a packet declares.
const DUBLIN_CORE: &str = "http://purl.org/dc/elements/1.1/";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const PDF: &str = "http://ns.adobe.com/pdf/1.3/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

// How large a packet is read, and how deep its elements nest.
const MAX_LENGTH: usize = 4 << 20;
const MAX_DEPTH: usize = 64;

impl Xmp {
    /// Reads `xml`, the text of an XMP packet. `None` if it isn't XML.
    pub fn parse(xml: &str) -> Option<Self> {
        let root = parse(xml)?;
        Some(Self {
            xml: xml.to_string(),
            root,
        })
    }

    /// The XML, as the PDF has it.
    pub fn xml(&self) -> &str {
        &self.xml
    }

    /// `dc:title`, in the default language or else the first one.
    pub fn title(&self) -> Option<String> {
        self.property(DUBLIN_CORE, "title")
    }

    /// `dc:creator`: the authors, separated by `; `.
    pub fn author(&self) -> Option<String> {
        self.property(DUBLIN_CORE, "creator")
    }

    /// `dc:description`, in the default language or else the first one.
    pub fn subject(&self) -> Option<String> {
        self.property(DUBLIN_CORE, "description")
    }

    /// `pdf:Keywords`, or else `dc:subject`, separated by `, `.
    pub fn keywords(&self) -> Option<String> {
        self.property(PDF, "Keywords")
            .or_else(|| self.property_joined(DUBLIN_CORE, "subject", ", "))
    }

    /// `xmp:CreatorTool`: the application that created the original document.
    pub fn creator(&self) -> Option<String> {
        self.property(XMP, "CreatorTool")
    }

    /// `pdf:Producer`: the application that converted it to PDF.
    pub fn producer(&self) -> Option<String> {
        self.property(PDF, "Producer")
    }

    /// `xmp:CreateDate`, if it can be read.
    pub fn creation_date(&self) -> Option<Date> {
        parse_xmp_date(&self.property(XMP, "CreateDate")?)
    }

    /// `xmp:ModifyDate`, if it can be read.
    pub fn modification_date(&self) -> Option<Date> {
        parse_xmp_date(&self.property(XMP, "ModifyDate")?)
    }

    fn property(&self, namespace: &str, name: &str) -> Option<String> {
        self.property_joined(namespace, name, "; ")
    }

    // The value of the property `name` of `namespace`, as an attribute or an
    // element of any `rdf:Description`: its text, the default language of an
    // `rdf:Alt`, or the items of an `rdf:Seq` or `rdf:Bag` joined by `glue`.
    fn property_joined(&self, namespace: &str, name: &str, glue: &str) -> Option<String> {
        let mut found = None;
        self.root.walk(&mut |node, scope| {
            if found.is_some() {
                return;
            }
            if let Some(value) = node.attribute_in(scope, namespace, name) {
                found = Some(value.to_string());
                return;
            }
            if node.is_named(scope, namespace, name) {
                found = node.value(scope, glue);
            }
        });
        found
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

/// Reads the XMP packet of the catalog `catalog`: its `/Metadata` stream,
/// decoded. `None` if there is none, or it can't be read or isn't XML.
pub fn read_xmp(resolver: &mut Resolver<'_>, catalog: &Dictionary) -> Option<Xmp> {
    let id = catalog.get("Metadata")?.as_reference()?;
    if !matches!(resolver.resolve(id).ok()?.as_ref(), Object::Stream(_)) {
        return None;
    }
    let data = resolver.stream_data(id).ok()?.data;
    if data.len() > MAX_LENGTH {
        return None;
    }
    let text = match data.as_slice() {
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(&data).into_owned(),
    };
    Xmp::parse(&text)
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| unit(*pair))
        .collect();
    String::from_utf16_lossy(&units)
}

// An element: its name, attributes, text and children.
#[derive(Debug, Clone, PartialEq, Default)]
struct Node {
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

// The namespaces declared around an element: prefix and URI, innermost last.
type Scope = Vec<(String, String)>;

impl Node {
    // Calls `visit` with every element, outermost first, and the namespaces
    // declared around it.
    fn walk(&self, visit: &mut dyn FnMut(&Node, &Scope)) {
        let mut scope = Scope::new();
        self.walk_in(&mut scope, visit);
    }

    fn walk_in(&self, scope: &mut Scope, visit: &mut dyn FnMut(&Node, &Scope)) {
        let declared = scope.len();
        for (key, value) in &self.attributes {
            if let Some(prefix) = key.strip_prefix("xmlns:") {
                scope.push((prefix.to_string(), value.clone()));
            } else if key == "xmlns" {
                scope.push((String::new(), value.clone()));
            }
        }
        visit(self, scope);
        for child in &self.children {
            child.walk_in(scope, visit);
        }
        scope.truncate(declared);
    }

    // Whether `qualified`, a name like `dc:title`, is `local` in `namespace`.
    fn names(scope: &Scope, qualified: &str, namespace: &str, local: &str) -> bool {
        let (prefix, name) = qualified.split_once(':').unwrap_or(("", qualified));
        name == local
            && scope
                .iter()
                .rev()
                .find(|(declared, _)| declared == prefix)
                .is_some_and(|(_, uri)| uri == namespace)
    }

    fn is_named(&self, scope: &Scope, namespace: &str, local: &str) -> bool {
        Self::names(scope, &self.name, namespace, local)
    }

    fn attribute_in(&self, scope: &Scope, namespace: &str, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.contains(':') && Self::names(scope, key, namespace, local))
            .map(|(_, value)| value.as_str())
    }

    // The value of a property element: the items of the array inside it, or
    // its text.
    fn value(&self, scope: &Scope, glue: &str) -> Option<String> {
        let array = self.children.iter().find(|child| {
            ["Alt", "Seq", "Bag"]
                .iter()
                .any(|kind| Self::names(scope, &child.name, RDF, kind))
        });
        let Some(array) = array else {
            return Some(self.text.clone());
        };
        let items: Vec<&Node> = array
            .children
            .iter()
            .filter(|item| Self::names(scope, &item.name, RDF, "li"))
            .collect();
        if Self::names(scope, &array.name, RDF, "Alt") {
            let default = items.iter().find(|item| {
                item.attributes
                    .iter()
                    .any(|(key, value)| key == "xml:lang" && value == "x-default")
            });
            return default.or(items.first()).map(|item| item.text.clone());
        }
        let texts: Vec<String> = items
            .iter()
            .map(|item| item.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect();
        (!texts.is_empty()).then(|| texts.join(glue))
    }
}

// Reads XML into a tree under an unnamed root. Declarations, processing
// instructions, comments and the doctype are skipped. `None` if a tag isn't
// closed or the elements nest too deep; end tags that don't match close
// their element anyway, as the packets of some writers need.
fn parse(xml: &str) -> Option<Node> {
    let mut stack = vec![Node::default()];
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(start) = rest.find('<') else {
            stack.last_mut()?.text.push_str(&unescape(rest));
            break;
        };
        stack.last_mut()?.text.push_str(&unescape(&rest[..start]));
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>")?;
            stack.last_mut()?.text.push_str(&after[..end]);
            rest = &after[end + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[rest.find('>')? + 1..];
        } else if let Some(after) = rest.strip_prefix("</") {
            rest = &after[after.find('>')? + 1..];
            if stack.len() > 1 {
                let done = stack.pop()?;
                stack.last_mut()?.children.push(done);
            }
        } else {
            let end = tag_end(rest)?;
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(tag) => (tag, true),
                None => (tag, false),
            };
            let node = element(tag)?;
            if empty {
                stack.last_mut()?.children.push(node);
            } else {
                if stack.len() > MAX_DEPTH {
                    return None;
                }
                stack.push(node);
            }
        }
    }
    while stack.len() > 1 {
        let done = stack.pop()?;
        stack.last_mut()?.children.push(done);
    }
    stack.pop()
}

// Where the tag that starts `text` ends, at its `>`, outside quotes.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in text.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

// An element from the text of its start tag: its name and attributes.
fn element(tag: &str) -> Option<Node> {
    let tag = tag.trim();
    let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
    let mut node = Node {
        name: tag[..name_end].to_string(),
        ..Node::default()
    };
    let mut rest = tag[name_end..].trim_start();
    while !rest.is_empty() {
        let equals = rest.find('=')?;
        let key = rest[..equals].trim().to_string();
        let after = rest[equals + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let close = after[1..].find(quote)? + 1;
        node.attributes.push((key, unescape(&after[1..close])));
        rest = after[close + 1..].trim_start();
    }
    Some(node)
}

// Text with its entities and character references replaced.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let character = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse))
                .and_then(|code| code.ok())
                .and_then(char::from_u32),
        };
        match character {
            Some(character) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A date of XMP, in ISO 8601: `YYYY`, `YYYY-MM`, `YYYY-MM-DD`, and then
/// `Thh:mm`, `Thh:mm:ss` or `Thh:mm:ss.s`, with a time zone, `Z` or `+hh:mm`,
/// after the time. `None` if it isn't one.
pub fn parse_xmp_date(text: &str) -> Option<Date> {
    let text = text.trim();
    let number = |part: &str, low: u32, high: u32| {
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
            .filter(|value| (low..=high).contains(value))
    };
    let (day, time) = text.split_once('T').unwrap_or((text, ""));
    let mut parts = day.split('-');
    let year = number(parts.next()?, 0, 9999)?;
    let month = parts.next().map_or(Some(1), |m| number(m, 1, 12))?;
    let day = parts.next().map_or(Some(1), |d| number(d, 1, 31))?;
    if parts.next().is_some() {
        return None;
    }
    let mut date = Date {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour: 0,
        minute: 0,
        second: 0,
        utc_offset: None,
    };
    if time.is_empty() {
        return Some(date);
    }
    let zone_at = time.find(['Z', '+', '-']).unwrap_or(time.len());
    let (clock, zone) = time.split_at(zone_at);
    let mut clock = clock.split(':');
    date.hour = number(clock.next()?, 0, 23)? as u8;
    date.minute = number(clock.next()?, 0, 59)? as u8;
    if let Some(seconds) = clock.next() {
        let whole = seconds.split('.').next().unwrap_or(seconds);
        date.second = number(whole, 0, 59)? as u8;
    }
    date.utc_offset = match zone {
        "" => None,
        "Z" => Some(0),
        zone => {
            let sign: i16 = if zone.starts_with('-') { -1 } else { 1 };
            let (hours, minutes) = zone[1..].split_once(':').unwrap_or((&zone[1..], "0"));
            let hours = number(hours, 0, 23)? as i16;
            let minutes = number(minutes, 0, 59)? as i16;
            Some(sign * (hours * 60 + minutes))
        }
    };
    Some(date)
}
