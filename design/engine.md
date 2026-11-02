# OZARU ENGINE: architecture and roadmap

Status: **proposal**. Nothing in this document is implemented yet.

OZARU ENGINE is ozaru's own PDF engine, written in Rust and independent of other PDF
engines. This document defines its architecture, the order in which it is built,
and how it replaces `pdf_oxide` without breaking the published library. The starting
point is described in [audit-0.1.md](audit-0.1.md).

## 1. Technical principles

| Principle | How it applies |
|---|---|
| **Rust processes, Python exposes** | The engine doesn't know about Python. The PyO3 layer only converts types, errors and warnings. |
| **No third-party PDF engines** | No `pdf_oxide`, `lopdf`, MuPDF or PDFium inside `ozaru-engine`. |
| **Helper crates only with a reason** | Compression, cryptography and TrueType font reading are allowed. Each one is approved in writing in this document. |
| **No `unsafe`** | `#![forbid(unsafe_code)]` across the whole engine. |
| **No global state** | No mutable `static` and no `thread_local!`, so one failure can't affect other reads. This is the lesson from problem P2 in the audit. |
| **`Send + Sync` types** | A `Document` can be shared between threads, so the Python layer no longer needs a thread per call. |
| **Untrusted input** | Every PDF is treated as hostile, with limits on depth, size and number of objects. |
| **Errors versus diagnostics** | An error stops the operation. A diagnostic describes a recoverable problem, such as an unreadable page or a repaired xref, and the engine keeps going. |
| **A panic is a bug** | Fuzzing must find zero panics. The Python layer keeps `catch_unwind` as a last line of defense. |
| **Nothing incomplete is presented as finished** | A feature is exposed in Python only when its phase meets its completion criteria. |

## 2. Repository layout

A Cargo workspace. The current crate stays the Python package, so `pyproject.toml`
and CI change as little as possible.

```text
ozaru/
├── Cargo.toml                  workspace + `ozaru` crate (PyO3 bindings)
├── src/lib.rs                  Python layer
├── crates/
│   └── ozaru-engine/
│       ├── Cargo.toml          no dependency on pyo3
│       ├── src/
│       │   ├── lib.rs
│       │   ├── base/           errors, limits and (later) diagnostics
│       │   ├── syntax/         lexer, tokens, objects, parser
│       │   ├── xref/           tables, streams, incremental updates, repair
│       │   ├── filters/        Flate, LZW, ASCII85, ASCIIHex, RunLength, predictors
│       │   ├── security/       Standard Security Handler
│       │   ├── document/       catalog, page tree, resources, Info
│       │   ├── fonts/          encodings, CMaps, ToUnicode, embedded fonts
│       │   ├── content/        operators and content interpreter
│       │   ├── text/           glyphs, lines, blocks, reading order
│       │   ├── image/          phase 10
│       │   ├── layout/         phase 9
│       │   └── writer/         phase 11
│       ├── tests/              integration tests with reference PDFs
│       ├── benches/            benchmarks
│       └── fuzz/               cargo-fuzz targets
└── corpus/                     reference PDFs and their licenses (MANIFEST)
```

While the Python layer doesn't depend on `ozaru-engine`, the engine **isn't
included in the wheels**, so nothing changes for users.

## 3. Components

| Module | Responsibility | Depends on |
|---|---|---|
| `base` | Shared base: errors with positions, limits and, later, diagnostics. It isn't called `core` because that is the name of Rust's own core library. | — |
| `syntax` | Bytes to tokens, and tokens to objects (`Object`) | base |
| `xref` | Find and resolve indirect objects | `syntax`, `filters` |
| `filters` | Decode stream contents | base |
| `security` | Decrypt strings and streams | `syntax`, RustCrypto |
| `document` | Catalog, inheritable page attributes (`Resources`, `MediaBox`, `Rotate`) and Info | `xref` |
| `fonts` | Character codes to Unicode, and glyph widths | `document`, `filters` |
| `content` | Operators, graphics state and text state | `syntax`, `fonts` |
| `text` | Positioned glyphs to characters, words, lines and blocks | `content` |
| `layout` | Columns, reading order and structure | `text` |
| `image` | Images and their decoding | `document`, `filters` |
| `writer` | Serialize and modify documents | `syntax`, `xref` |

### Object model

```rust
pub enum Object {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(PdfString),        // bytes + whether it was literal or hexadecimal
    Name(Name),               // bytes with #xx escapes already decoded
    Array(Vec<Object>),
    Dictionary(Dictionary),   // keeps insertion order
    Stream(Stream),           // dictionary + range of raw, undecoded bytes
    Reference(ObjectId),      // object number + generation
}
```

- **Strings and names** are stored as bytes. Turning them into text is the job of
  `fonts` and `text`, not the parser.
- **Streams aren't decoded while parsing.** They are decoded on demand.

## 4. Roadmap by phase

The roadmap follows the planned order, with two justified changes:

1. **Phases 3 and 4:** xref streams in PDF 1.5+ are usually compressed with Flate and
   a PNG predictor. So phase 3 includes a minimal Flate and predictor, and phase 4
   completes the filters.
2. **Encryption:** it wasn't in the planned list. It goes in phase 4, because without
   decryption not even a string of an encrypted PDF can be read. User-facing
   passwords and permissions come later.

Each phase ends when its criteria are met and reviewed. The fuzzing time and
similarity figures are proposals: **the project owner decides the final values**.

### Phase 1: PDF parser (syntax)

- **Goal:** turn bytes into direct PDF objects, following ISO 32000-2 §7.2–7.3.
- **Components:** lexer, tokens, objects, direct object parser, errors and limits.
- **Files:** `syntax/{lexer,token,object,parser}.rs` and `base/{error,limits}.rs`.
- **Allowed dependencies:** none.
- **Tests:**
  - Cases from the standard for each object type.
  - Edge cases.
  - Properties: the parser never panics on random bytes.
  - Fuzzing.
- **Done when:** the criteria in section 6 are met.

### Phase 2: Indirect objects and references

- **Goal:** parse `n g obj … endobj`, `n g R` and streams with their `/Length`,
  including an indirect `/Length`.
- **Components:** `ObjectId` and the indirect object parser. If `/Length` is wrong,
  the stream is found by searching for `endstream`, with a diagnostic.
- **Files:** `syntax/indirect.rs`.
- **Allowed dependencies:** none.
- **Tests:** correct, wrong, indirect and missing `/Length`; objects without
  `endobj`; generations other than 0.
- **Done when:**
  - Every object in the corpus is parsed or gives an error with its position.
  - Fuzzing finds 0 panics.

### Phase 3: Xref and document loading

- **Goal:** open a document and resolve any reference.
- **Components:**
  - `startxref`, classic xref tables and `trailer`.
  - Xref streams (`/W`, `/Index`) and object streams (`ObjStm`).
  - Chains of incremental updates (`/Prev`) and hybrid files (`XRefStm`).
  - Repair: rebuild the xref by scanning for `n g obj`, with a diagnostic.
  - Reference cycle detection.
  - Cache of resolved objects.
- **Files:** `xref/{table,stream,objstm,repair,resolver}.rs` and
  `filters/{flate,predictor}.rs`, in a minimal version.
- **Allowed dependencies:** `miniz_oxide`, only to inflate Flate data. It's pure Rust
  without `unsafe`, and decompression isn't a PDF engine.
- **Tests:**
  - PDFs from 1.0 to 2.0, classic tables, xref streams, several incremental updates,
    ObjStm and files with wrong offsets.
  - Reference cycles.
- **Done when:**
  - It opens 100% of the valid corpus.
  - It repairs the damaged-xref cases in the corpus.
  - The object count matches the reference.

### Phase 4: Streams, filters and decryption

- **Goal:** decode every stream needed for text.
- **Components:**
  - Filters: FlateDecode, LZWDecode (with `EarlyChange`), ASCIIHexDecode,
    ASCII85Decode and RunLengthDecode.
  - PNG and TIFF predictors, chained filters and `DecodeParms`.
  - Standard Security Handler R2 to R6, with 40/128-bit RC4 and AES-128/256, and
    an empty user password.
  - Limit on decompressed size, against decompression bombs.
- **Files:** `filters/*.rs` and `security/{standard,rc4,aes}.rs`.
- **Allowed dependencies:**
  - `miniz_oxide`.
  - RustCrypto: `aes`, `cbc`, `md-5`, `sha2` and `rc4`. Cryptography is not
    reimplemented.
- **Tests:**
  - Test vectors for each filter.
  - Encrypted PDFs for each revision.
  - Decompression bomb: must give an error and not run out of memory.
- **Done when:**
  - It decodes every content stream in the corpus.
  - It decrypts the corpus encrypted with an empty password.
  - The size limit works.

### Phase 5: Catalog and page tree

- **Goal:** list the pages and their properties.
- **Components:**
  - Catalog and `/Pages` tree with attribute inheritance.
  - `MediaBox`, `CropBox` and `Rotate`.
  - Resources and the `Info` dictionary.
  - Protection against cyclic trees.
- **Files:** `document/{catalog,pages,resources,info}.rs`.
- **Allowed dependencies:** none.
- **Tests:** deep, unbalanced and cyclic trees, and trees with a wrong `/Count`.
- **Done when:**
  - `page_count` matches `pdf_oxide` and pypdf on the whole corpus.
  - At this point, the published `page_count` can start using the engine.

### Phase 6: Fonts and Unicode

- **Goal:** map character codes to Unicode and get the glyph widths.
- **Components:**
  - The standard 14 fonts, with their AFM metrics.
  - Type1, TrueType and Type3, and Type0 with CIDFont.
  - Standard, WinAnsi, MacRoman, PDFDoc, Symbol and ZapfDingbats encodings, and
    `/Differences`.
  - Adobe Glyph List.
  - Embedded and predefined CMaps, and ToUnicode.
  - When ToUnicode is missing, the `cmap` table of the embedded font.
- **Files:** `fonts/{encoding,glyphlist,cmap,tounicode,simple,composite,metrics}.rs`
  and `fonts/data/`.
- **Allowed dependencies:** `ttf-parser`, to read the tables of embedded TrueType and
  OpenType fonts. It reads tables and doesn't interpret PDF.
- **Data to check:** the licenses of the Adobe Glyph List, the AFM metrics of the 14
  fonts and Adobe's CMaps.
- **Tests:** each encoding, ToUnicode with `bfchar` and `bfrange`, CID with
  Identity-H, and fonts without ToUnicode.
- **Done when:** the codes of each font in the corpus map like the reference, at the
  percentage set by the project owner.

### Phase 7: Content interpretation

- **Goal:** run content streams.
- **Components:**
  - Graphics state operators (`q`, `Q`, `cm`) and text operators (`BT`, `ET`, `Tf`,
    `Tm`, `Td`, `TD`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, `Ts`, `Tr`).
  - Text-showing operators (`Tj`, `TJ`, `'`, `"`).
  - Form XObjects and marked content (`BDC`, `EMC`, `ActualText`).
  - Unknown operators are ignored, with a diagnostic.
- **Files:** `content/{operators,interpreter,graphics_state,text_state}.rs`.
- **Allowed dependencies:** none.
- **Tests:** each operator, nested and recursive XObjects, and truncated content.
- **Done when:** every glyph in the corpus has its position, font and size.

### Phase 8: Text extraction

- **Goal:** plain text equivalent to what ozaru publishes today.
- **Components:**
  - Glyphs to characters.
  - Inferred spaces, from gaps and `TJ` adjustments.
  - Line breaks and basic line ordering.
  - Pages separated by `\f`, as today.
- **Files:** `text/{glyph,word,line,extract}.rs`.
- **Allowed dependencies:** `unicode-normalization` if needed for ligatures, with a
  reason.
- **Tests:**
  - Comparison with `pdf_oxide`, pypdf and PyMuPDF on the corpus, after normalizing
    whitespace.
  - The documentation examples must give the same output.
- **Done when:** it meets the migration criteria in section 5.

### Phase 9: Coordinates, blocks and layout

- **Goal:** give access to the structure of the text.
- **Components:**
  - Characters with their bounding boxes.
  - Lines, blocks and columns.
  - Multi-column reading order.
  - RTL, with `unicode-bidi` if needed.
  - Vertical writing.
- **Files:** `text/{bbox,block}.rs` and `layout/*.rs`.
- **Allowed dependencies:** `unicode-bidi`, with a reason.
- **Tests:** two-column documents, simple tables, Arabic and Hebrew, and vertical CJK.
- **Done when:** coordinates match PyMuPDF's within a tolerance, and the order is
  correct on the layout corpus.

### Phase 10: Images and metadata

- **Goal:** extract images and metadata.
- **Components:**
  - Image XObjects and inline images.
  - Basic color spaces.
  - DCT (JPEG) and CCITT.
  - XMP and Info.
- **Allowed dependencies:** a JPEG decoder such as `zune-jpeg`, and a separate
  evaluation of JPX and JBIG2 because of their complexity and risk.
- **Done when:** the images in the corpus match the reference pixel for pixel, for
  the supported formats.

### Phase 11: Document manipulation

- **Goal:** create and modify documents.
- **Components:**
  - Object serializer.
  - Writing xrefs and incremental updates.
  - Merging, splitting and rotating pages.
- **Tests:**
  - Read, write and read again without losing information.
  - Validation with external tools, in tests only.
- **Done when:** the documents it produces open without errors in the reference
  readers.

### Phase 12: Rendering and advanced features

- **Goal:** render pages to images, and advanced features.
- **Components:**
  - Rasterizing paths and text.
  - Forms, annotations and user passwords.
  - Permissions.
- It is planned once phases 1 to 11 are closed.

## 5. Migration strategy

1. **Phases 1 to 4:** the engine grows on its own, as a workspace crate, and isn't
   included in the wheels.
2. **Phases 5 to 8:** a comparison test reads each corpus PDF with `pdf_oxide` and
   with the engine, and compares the results. pypdf and PyMuPDF are also used as
   references, but only in tests and never as distributed dependencies.
3. **Engine switch:** one function at a time, in this order:
   1. `page_count`, at the end of phase 5.
   2. `extract_text`, at the end of phase 8.
   3. Once the last use is gone, `pdf_oxide` is removed from `Cargo.toml`.

   **Proposed criteria** for switching `extract_text`:
   - Zero panics and zero timeouts on the corpus.
   - Normalized text with a similarity of 0.98 or more to the reference, on at least
     95% of the documents.
   - No corpus document loses a page that the published version reads.
4. **Release:** each engine switch is a `0.x.0` version with a clear changelog note,
   because spaces and line breaks may change.

The current public API (`extract_text`, `page_count` and `PageWarning`) is kept
throughout the migration.

## 6. First milestone: syntax parser

**Scope:** phase 1 only. Turn bytes into direct `Object`s, without xrefs, streams or
indirect objects (`n g R` references are recognized). Nothing is exposed in Python.

### Files

```text
crates/ozaru-engine/
├── Cargo.toml              edition 2024, no dependencies
├── src/lib.rs              #![forbid(unsafe_code)], public modules
├── src/base/error.rs       Error { kind, offset }, ErrorKind
├── src/base/limits.rs      Limits { max_depth, max_string_length, ... }
├── src/syntax/mod.rs
├── src/syntax/token.rs     Token and its byte range
├── src/syntax/lexer.rs     Lexer<'a> over &[u8]
├── src/syntax/object.rs    Object, PdfString, Name, Dictionary, ObjectId
├── src/syntax/parser.rs    parse_object(&[u8], &Limits) -> Result<(Object, usize)>
├── tests/                  one file per module, apart from the code
└── fuzz/fuzz_targets/parse_object.rs
```

### Behavior, following ISO 32000-2 §7.2–7.3

| Element | Rules |
|---|---|
| Whitespace | NUL, HT, LF, FF, CR and SP. An EOL can be CR, LF or CR+LF. |
| Delimiters | `( ) < > [ ] { } / %` |
| Comments | From `%` to the end of the line, ignored. |
| Booleans and null | `true`, `false` and `null`. |
| Numbers | `123`, `-17`, `+5`, `0.5`, `.5`, `4.`, `-.002`. An integer overflow is an error, not a truncated value. |
| Literal strings | Nested parentheses, escapes `\n \r \t \b \f \( \) \\` and octal `\d`, `\dd` and `\ddd`. A backslash before an EOL continues the line. An unescaped EOL is normalized to LF. |
| Hexadecimal strings | Whitespace is ignored. With an odd number of digits, a 0 is appended. A non-hex character is an error. |
| Names | `/Name`, with `#xx` escapes. `/` alone is a valid empty name. |
| Arrays and dictionaries | Nesting is limited by `max_depth`, capped at `Limits::MAX_DEPTH` (1000): dropping, cloning or printing a deeper object would recurse and could overflow the stack. The parser itself uses an explicit stack, not recursion. Dictionary keys must be names, and a repeated key keeps the last value, with a diagnostic. |
| References | `n g R` is recognized with a two-token lookahead. |

### Completion criteria

| Criterion | How it's checked |
|---|---|
| Each rule in the table above has at least one test | Review of the files in `tests/` |
| No panic on random bytes | A property test over 100,000 inputs |
| Line coverage of 90% or more in `syntax/` | `cargo llvm-cov` |
| Fuzzing without failures | 1 hour locally and 60 seconds in every CI run |
| Limits work | Tests with 10,000 levels of nesting and huge strings that give an error without overflowing the stack |
| No `unsafe` and no dependencies | `#![forbid(unsafe_code)]` and `cargo tree` |
| The wheels don't change | The Python layer doesn't depend on the engine yet |
| CI | `cargo test`, `cargo clippy -D warnings` and `cargo fmt --check` for the workspace, on Linux, macOS and Windows |

### Out of scope for this milestone

Xref, indirect objects with `obj`/`endobj`, streams, filters, encryption and any
Python API.
