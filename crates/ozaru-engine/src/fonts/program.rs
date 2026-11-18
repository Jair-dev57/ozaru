use std::sync::Arc;

use super::embedded::{ProgramKind, font_program};
use super::resolved;
use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// The embedded font program of a font, kept to draw its glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderProgram {
    pub kind: ProgramKind,
    pub data: Vec<u8>,
    /// For a CIDFont with TrueType outlines, the glyph of each CID
    /// (`/CIDToGIDMap`): the CID itself if `None`.
    pub cid_to_gid: Option<Vec<u16>>,
}

/// Reads the font program of the font dictionary `font`: its descriptor's
/// `/FontFile`, `/FontFile2` or `/FontFile3`, or for a composite font that of
/// its descendant font. `None` if the font has no program in the file.
pub fn read_render_program(
    resolver: &mut Resolver<'_>,
    font: &Dictionary,
) -> Option<Arc<RenderProgram>> {
    let mut cid_to_gid = None;
    let mut holder = font.clone();
    let subtype = font.get("Subtype").map(|entry| resolved(resolver, entry));
    if subtype
        .as_deref()
        .and_then(Object::as_name)
        .is_some_and(|name| name.as_bytes() == b"Type0")
    {
        let descendants = resolved(resolver, font.get("DescendantFonts")?);
        let first = resolved(resolver, descendants.as_array()?.first()?);
        holder = first.as_dictionary()?.clone();
        if let Some(entry) = holder.get("CIDToGIDMap")
            && let Object::Reference(id) = entry
            && let Ok(object) = resolver.resolve(*id)
            && object.as_stream().is_some()
            && let Ok(decoded) = resolver.stream_data(*id)
        {
            cid_to_gid = Some(
                decoded
                    .data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect(),
            );
        }
    }
    let descriptor = resolved(resolver, holder.get("FontDescriptor")?);
    let (kind, data) = font_program(resolver, descriptor.as_dictionary()?)?;
    Some(Arc::new(RenderProgram {
        kind,
        data,
        cid_to_gid,
    }))
}
