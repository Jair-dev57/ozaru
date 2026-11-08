# OZARU ENGINE: architecture and roadmap

Status: phases 1 to 5 are implemented in `crates/ozaru-engine`, and phase 6 is in
progress. The rest is a proposal. See [Progress](#progress).

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
│       │   ├── base/           errors, diagnostics and limits
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
│       │   └── corpus/         generated reference PDFs, their MANIFEST and the
│       │                       page counts of other readers
│       ├── benches/            benchmarks
│       └── fuzz/               cargo-fuzz targets
```

The corpus is in the engine's tests rather than at the root, because the root
crate packages every file at the root into the `ozaru` source distribution.

While the Python layer doesn't depend on `ozaru-engine`, the engine **isn't
included in the wheels**, so nothing changes for users.

## 3. Components

| Module | Responsibility | Depends on |
|---|---|---|
| `base` | Shared base: errors with positions, diagnostics (capped by `Limits::max_diagnostics`) and limits. It isn't called `core` because that is the name of Rust's own core library. | — |
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

- **Goal:** parse `n g obj … endobj`, `n g R` and streams with their `/Length`.
  An indirect `/Length` is recognized, but resolving it needs the xref table, so
  that moves to phase 3. Until then its stream ends at `endstream`, with an
  `UnresolvedStreamLength` diagnostic.
- **Components:** `ObjectId`, diagnostics, `Stream`, `IndirectObject` and the
  indirect object parser. If `/Length` is wrong, the stream is found by searching
  for `endstream`, with a diagnostic.
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
- **Files:** `xref/{startxref,table,stream,chain,objstm,repair,resolver}.rs` and
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
  fonts and Adobe's CMaps. The Adobe Glyph List is BSD-3 (checked in 6.2).
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

## Progress

| Phase | State | Notes |
|---|---|---|
| 1. Syntax | Done | Lexer, objects and parser. Fuzzed, 90%+ line coverage. |
| 2. Indirect objects and streams | Done | Every object of the repository's 8 real PDFs is read or reported. |
| 3. Xref and document loading | Done | `startxref`, classic tables, xref streams, `/Prev` chains, hybrid files, ObjStm, Flate with PNG predictors, a cached resolver with cycle detection and indirect `/Length`, and repair by scanning. See the review below. |
| 4. Streams, filters and decryption | Done | Every filter for text (Flate, LZW, RunLength, ASCIIHex, ASCII85) with the TIFF and PNG predictors, and the standard security handler from revision 2 to 6 (RC4, AES-128, AES-256), decrypting on the fly in the resolver. See the review below. |
| 5. Catalog and page tree | Done | The catalog, with fallbacks for damaged files; the page tree, walked with its own stack, skipping cycles and keeping unreadable pages as damaged; inherited boxes, rotation and resources; the `Info` dictionary; and `Document`, the entry point. `page_count` uses it since 0.2.0. See the review below. |
| 6. Fonts and Unicode | In progress | 6.1: the Standard, MacRoman, WinAnsi, Symbol and ZapfDingbats encodings, transcribed from Annex D and checked code by code against the Adobe AFM files of the standard 14 fonts and pdfminer's table, and `/Encoding` with `/BaseEncoding` and `/Differences`. MacExpertEncoding isn't supported yet: it falls back to the built-in encoding with a diagnostic. PDFDocEncoding isn't a font encoding; it is decoded by `decode_text_string` since phase 5. 6.2: glyph names to Unicode with the Adobe Glyph List and the ITC Zapf Dingbats Glyph List, following the rules of the Adobe Glyph List Specification (suffixes, ligatures with underscores, `uniXXXX` and `uXXXX`). Both lists are BSD-3 and are in `fonts/data/` unmodified, with their notice in `THIRD_PARTY_NOTICES.md`, which the wheels include. |

### Phase 3 review

Checked against the completion criteria of phase 3, on the 10 PDFs of the
repository: 8 written by ReportLab (PDF 1.3 and 1.4) and 2 rewritten by
Ghostscript 10.06 (PDF 1.7, one with an xref stream and an ObjStm).

| Criterion | Result |
|---|---|
| Opens 100% of the valid corpus | Met: the 6 valid PDFs open, and every object resolves without diagnostics. |
| Repairs the damaged-xref cases | Met: `damaged.pdf` and `page_panic.pdf`, whose tables have corrupted entries, open after a rebuild. `damaged.pdf` also has its page tree header damaged on purpose, so that object can't be found. |
| The object count matches the reference | Partly: it matches `/Size` and a rebuild by scanning, which finds exactly the same objects. No external reader has been compared yet. |

Known gaps, to close with the larger corpus planned for phase 5:

- **Incremental updates and hybrid files** are only tested with files written by
  the tests, because no PDF of the repository has them and Ghostscript can't
  write them. Since phase 5.7, the corpus has a file with two incremental
  updates, which pypdf and Poppler read the same way, but it is still written by
  the corpus generator. Hybrid files are still only tested synthetically.
- **PDF 1.0 to 1.2 and 2.0** files aren't in the corpus. Since phase 5.7, they
  are: 1.0 from the corpus generator, 1.2 and 2.0 from Ghostscript.
- **Compressed objects can't be the catalog found by repair**: when no trailer has a
  `/Root`, only catalogs outside object streams are looked for. Since phase 5.1,
  `read_catalog` searches object streams too, when the trailer's `/Root` can't be
  used.

### Phase 4 review

Checked against the completion criteria of phase 4, on the 13 PDFs of the
repository: the 10 of phase 3 and 3 encrypted rewrites of the sample report by
Ghostscript 10.06 (RC4 40-bit, RC4 128-bit with an xref stream and an ObjStm, and
RC4 128-bit with a user password).

| Criterion | Result |
|---|---|
| Decodes every content stream in the corpus | Met: every page of the valid PDFs decodes without diagnostics, and the encrypted rewrites give exactly the same content as the unencrypted one. |
| Decrypts the corpus encrypted with an empty password | Met: `owner_password.pdf` and the two Ghostscript files with an empty user password. The one with the user password `user` opens after `unlock`. |
| The size limit works | Met: Flate, LZW, RunLength and ASCII85 stop at `max_decoded_length`, each with a test. |

Known gaps:

- **No real AES file is in the corpus**, because no tool at hand writes them. AES-128
  and AES-256 are checked against vectors from OpenSSL and an independent Python
  implementation. Since phase 5.7, the corpus has AES-128 and AES-256 files
  written by qpdf, and every page of them decrypts.
- **Only the user password is checked.** Opening with the owner password, and the
  `/Perms` check of revisions 5 and 6, aren't done. Revision 6 passwords are used
  as UTF-8 without SASLprep, which only matters for some non-ASCII passwords.
  Since 0.2.4, the owner password opens files too (algorithms 7 and 12), checked
  on real files of revisions 2, 3, 4 and 6 and on a vector from an independent
  implementation. The `/Perms` check and SASLprep are still missing.
- **The `/Crypt` stream filter** isn't supported.
- **Repair can't read the object streams of an encrypted file**, because it doesn't
  decrypt. It only matters for files that are both encrypted and damaged.
- **A locked file with its page tree inside an object stream** can't be read until
  unlocked, since the object stream itself is encrypted.

### Phase 5 review

Checked against the completion criteria of phase 5, on the 31 PDFs of the
repository: the 13 of phase 4 and the 18 of the new generated corpus in
`crates/ozaru-engine/tests/corpus`, made with ReportLab, Ghostscript, qpdf and a
small writer of its own (see its `MANIFEST.md`).

| Criterion | Result |
|---|---|
| `page_count` matches `pdf_oxide` and pypdf on the whole corpus | Met, and Poppler's `pdfinfo` too: wherever the references agree, the engine gives the same count. In the 3 files where they disagree, the engine agrees with the majority, or with `pdf_oxide` when pypdf and Poppler can't open the file. `tests/corpus.rs` repeats the comparison in every `cargo test`. |
| Deep, unbalanced and cyclic trees, and trees with a wrong `/Count` | Met: unit tests for each, property tests with random trees, a 20,000-level tree, and a `document` fuzz target that ran 192,000 inputs without a failure. |
| Page attributes | Met: the media box, crop box and rotation of 283 pages match pypdf. |

The published `page_count` switched to the engine in 0.2.0, **without a fallback
to `pdf_oxide`**, as the project owner decided on 2026-11-06: a PDF the engine
can't read is fixed in the engine. Its errors keep their exception types, with
new messages.

Known gaps:

- **Speed on large files.** The engine reads every page object, while `pdf_oxide`
  seems to read only the root's `/Count`. Measured with release builds, a 3-page
  file takes 0.08 ms against 0.10 ms, but 500 pages take 3.7 ms against 1.0 ms,
  and 1000 pages in object streams 5.2 ms against 0.7 ms. About 2.6 µs go into
  parsing each object, so the lexer and parser are the place to speed up, which
  phases 6 to 8 will need anyway.
- **A locked file with its page tree in an encrypted object stream** can't be
  counted without its password, as the phase 4 review says. `page_count` raises a
  `ValueError` that says so, as `pdf_oxide` did.
- **An encrypted file with its information dictionary held by the trailer**, not
  as an indirect object, gives no information, since the resolver only decrypts
  indirect objects.
- **`tests/damaged.pdf`**, whose page tree object is destroyed, can't be read. No
  reference reader can read it either. Rebuilding a page tree from loose
  `/Type /Page` objects, as some readers do, isn't done. Since 0.2.5, it is:
  when the root of the page tree is destroyed, `rebuild_pages` takes every
  `/Type /Page` object in file order, with what it inherits from the nodes that
  survive, and keeps unreadable page objects as damaged pages. The order is a
  guess, which the `PageTreeRebuilt` diagnostic says. A file with no catalog at
  all isn't rebuilt yet.
- **Hybrid files** are still only tested with files written by the tests.

### Performance work (0.2.1)

`crates/ozaru-engine/benches/engine.rs` measures the engine with the standard
library only: `cargo bench -p ozaru-engine`. The baseline, on the development
laptop with a release build, before any optimization:

| Measurement | Time |
|---|---|
| Lexer, `many-pages.pdf` | 0.037 µs per token |
| Parser, one page dictionary | 1.8 µs |
| Resolver, every object of `many-pages.pdf` | 3.0 µs per object |
| `Document::open`, `report.pdf` (3 pages) | 27 µs |
| `Document::open`, `many-pages.pdf` (500 pages) | 3.0 ms |
| `Document::open`, `balanced-tree-compressed.pdf` (1000 pages) | 4.4 ms |

The median varied by 10 to 20% between runs, so the benchmark now reports the
fastest run, which varies by about 1%.

Results for 0.2.1, each change measured and committed on its own:

| Change | Effect |
|---|---|
| Keyword tokens point into the input instead of being copied | Lexer about 20% faster |
| Names of up to 22 bytes are stored inline | Allocations of a page dictionary from 19 to 8; documents about 17% faster |
| Integers of up to 18 digits are read in one pass | Lexer about 7% faster |
| Page attributes are read in place | `many-pages.pdf` about 10% faster |
| The cross-reference table is built at once | `Resolver::open` from 168 to 80 µs |
| A cache reserved up front in the resolver | No measurable effect, dropped |

`page_count` from Python, against ozaru 0.1.7 (`pdf_oxide`) and pypdf 7.0.0:

| File | 0.1.7 | 0.2.0 | 0.2.1 | pypdf |
|---|---|---|---|---|
| `report.pdf` (3 pages) | 0.094 ms | 0.080 ms | 0.066 ms | 0.63 ms |
| `many-pages.pdf` (500 pages) | 0.86 ms | 3.63 ms | 2.24 ms | 63 ms |
| `balanced-tree.pdf` (1000 pages) | 0.87 ms | 4.02 ms | 2.50 ms | 90 ms |
| `balanced-tree-compressed.pdf` (1000 pages) | 0.74 ms | 5.19 ms | 3.94 ms | 61 ms |

The project owner chose to release this and come back to the parser later,
since the engine is already much faster than pypdf. What is left is spread
through the core of the parser: moving large `Result`s between functions,
lookahead in a `VecDeque`, and dictionaries and arrays that grow one entry at a
time. `pdf_oxide` seems to read only the root's `/Count`, so matching it means
reading each page in under about 1.5 µs. Phases 6 to 8 will need a faster
parser anyway.

## 5. Migration strategy

1. **Phases 1 to 4:** the engine grows on its own, as a workspace crate, and isn't
   included in the wheels.
2. **Phases 5 to 8:** a comparison test reads each corpus PDF with `pdf_oxide` and
   with the engine, and compares the results. pypdf and PyMuPDF are also used as
   references, but only in tests and never as distributed dependencies.
3. **Engine switch:** one function at a time, in this order:
   1. `page_count`, at the end of phase 5. Done in 0.2.0, with no fallback to
      `pdf_oxide`.
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
├── src/syntax/parser.rs    parse_object(&[u8], Limits) -> Result<ParsedObject>
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
| Arrays and dictionaries | Nesting is limited by `max_depth`, capped at `Limits::MAX_DEPTH` (1000): dropping, cloning or printing a deeper object would recurse and could overflow the stack. The parser itself uses an explicit stack, not recursion. Dictionary keys must be names, and a repeated key keeps the last value, with a `DuplicateDictionaryKey` diagnostic. |
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
