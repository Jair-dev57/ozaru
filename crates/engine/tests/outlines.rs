//! Glyph outlines from font programs made here, byte by byte: the operators of
//! Type 2 and Type 1 charstrings, and the tables of TrueType fonts.

use ozaru_engine::fonts::outline::{Cff, TrueType, Type1};
use ozaru_engine::render::Segment;

// ---- CFF ----

fn num(v: i32) -> Vec<u8> {
    if (-107..=107).contains(&v) {
        vec![(v + 139) as u8]
    } else {
        let v = v as i16;
        vec![28, (v >> 8) as u8, v as u8]
    }
}

// Operands and operators: `ops!(10, 20, "rmoveto")`-like, by hand.
fn cs(parts: &[&[i32]], operators: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for (numbers, operator) in parts.iter().zip(operators) {
        for &n in *numbers {
            out.extend(num(n));
        }
        out.extend_from_slice(operator);
    }
    out
}

fn index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![(items.len() >> 8) as u8, items.len() as u8];
    if items.is_empty() {
        return out;
    }
    out.push(2);
    let mut offset = 1usize;
    out.extend([(offset >> 8) as u8, offset as u8]);
    for item in items {
        offset += item.len();
        out.extend([(offset >> 8) as u8, offset as u8]);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

// An operand of 5 bytes that holds a number the layout needs to know in
// advance.
fn fixed(v: usize) -> Vec<u8> {
    vec![
        29,
        (v >> 24) as u8,
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ]
}

// A CFF font whose glyphs are `charstrings`, after .notdef, with the given
// local and global subroutines. The glyphs are named as the Standard strings
// do, from "space".
fn cff(charstrings: &[Vec<u8>], local: &[Vec<u8>], global: &[Vec<u8>]) -> Vec<u8> {
    let mut all = vec![vec![14u8]];
    all.extend_from_slice(charstrings);
    let header = vec![1u8, 0, 4, 2];
    let names = index(&[b"Test".to_vec()]);
    let strings = index(&[]);
    let globals = index(global);
    let chars = index(&all);
    let locals = index(local);
    // The Private DICT: its Subrs offset is its own length, 7 bytes.
    let private_length = 6;
    let mut private = Vec::new();
    private.extend(fixed(private_length));
    private.push(19);
    assert_eq!(private.len(), private_length);
    let top_length = |_: ()| 5 + 1 + 5 + 5 + 1;
    let top_index_size = 2 + 1 + 4 + top_length(());
    let start = header.len() + names.len() + top_index_size + strings.len() + globals.len();
    let chars_at = start;
    let private_at = chars_at + chars.len();
    let mut top = Vec::new();
    top.extend(fixed(chars_at));
    top.push(17);
    top.extend(fixed(private_length));
    top.extend(fixed(private_at));
    top.push(18);
    assert_eq!(top.len(), top_length(()));
    let top = index(&[top]);
    let mut out = header;
    out.extend(names);
    out.extend(top);
    out.extend(strings);
    out.extend(globals);
    out.extend(chars);
    out.extend(private);
    out.extend(locals);
    out
}

fn path_of(font: &[u8], glyph: u16) -> Vec<Segment> {
    let cff = Cff::parse(font).unwrap();
    cff.outline(glyph, &|code| match code {
        65 => Some(9),
        66 => Some(10),
        _ => None,
    })
    .unwrap()
    .segments
}

const RMOVETO: &[u8] = &[21];
const RLINETO: &[u8] = &[5];
const HLINETO: &[u8] = &[6];
const VLINETO: &[u8] = &[7];
const RRCURVETO: &[u8] = &[8];
const ENDCHAR: &[u8] = &[14];

#[test]
fn lines_moves_and_the_width_of_a_cff_charstring() {
    let font = cff(
        &[
            cs(
                &[&[100, 100], &[50, 0, 0, 50], &[]],
                &[RMOVETO, RLINETO, ENDCHAR],
            ),
            // A width before the first operator is dropped.
            cs(&[&[7, 10, 10], &[20, 0], &[]], &[RMOVETO, RLINETO, ENDCHAR]),
            // hlineto and vlineto take turns.
            cs(&[&[0, 0], &[10, 20, 30], &[]], &[RMOVETO, HLINETO, ENDCHAR]),
            cs(&[&[0, 0], &[10, 20, 30], &[]], &[RMOVETO, VLINETO, ENDCHAR]),
        ],
        &[],
        &[],
    );
    assert_eq!(
        path_of(&font, 1),
        [
            Segment::Move(100.0, 100.0),
            Segment::Line(150.0, 100.0),
            Segment::Line(150.0, 150.0),
            Segment::Close
        ]
    );
    assert_eq!(
        path_of(&font, 2),
        [
            Segment::Move(10.0, 10.0),
            Segment::Line(30.0, 10.0),
            Segment::Close
        ]
    );
    assert_eq!(
        path_of(&font, 3),
        [
            Segment::Move(0.0, 0.0),
            Segment::Line(10.0, 0.0),
            Segment::Line(10.0, 20.0),
            Segment::Line(40.0, 20.0),
            Segment::Close
        ]
    );
    assert_eq!(
        path_of(&font, 4),
        [
            Segment::Move(0.0, 0.0),
            Segment::Line(0.0, 10.0),
            Segment::Line(20.0, 10.0),
            Segment::Line(20.0, 40.0),
            Segment::Close
        ]
    );
}

#[test]
fn the_curve_operators_of_type_2_charstrings() {
    let font = cff(
        &[
            // rrcurveto.
            cs(
                &[&[0, 0], &[10, 0, 10, 10, 0, 10], &[]],
                &[RMOVETO, RRCURVETO, ENDCHAR],
            ),
            // hhcurveto with a y at the start: dy1 dxa dxb dyb dxc.
            cs(
                &[&[0, 0], &[5, 10, 20, 30, 40], &[]],
                &[RMOVETO, &[27], ENDCHAR],
            ),
            // vvcurveto with an x at the start.
            cs(
                &[&[0, 0], &[5, 10, 20, 30, 40], &[]],
                &[RMOVETO, &[26], ENDCHAR],
            ),
            // hvcurveto and vhcurveto, one curve each, the last with an extra.
            cs(
                &[&[0, 0], &[10, 20, 30, 40], &[]],
                &[RMOVETO, &[31], ENDCHAR],
            ),
            cs(
                &[&[0, 0], &[10, 20, 30, 40, 50], &[]],
                &[RMOVETO, &[30], ENDCHAR],
            ),
            // rcurveline and rlinecurve.
            cs(
                &[&[0, 0], &[1, 2, 3, 4, 5, 6, 7, 8], &[]],
                &[RMOVETO, &[24], ENDCHAR],
            ),
            cs(
                &[&[0, 0], &[7, 8, 1, 2, 3, 4, 5, 6], &[]],
                &[RMOVETO, &[25], ENDCHAR],
            ),
        ],
        &[],
        &[],
    );
    assert_eq!(
        path_of(&font, 1),
        [
            Segment::Move(0.0, 0.0),
            Segment::Curve(10.0, 0.0, 20.0, 10.0, 20.0, 20.0),
            Segment::Close
        ]
    );
    // hhcurveto: dy1 = 5, then (10, 0) (20, 30) (40, 0).
    assert_eq!(
        path_of(&font, 2),
        [
            Segment::Move(0.0, 0.0),
            Segment::Curve(10.0, 5.0, 30.0, 35.0, 70.0, 35.0),
            Segment::Close
        ]
    );
    // vvcurveto: dx1 = 5, then (0, 10) (20, 30) (0, 40).
    assert_eq!(
        path_of(&font, 3),
        [
            Segment::Move(0.0, 0.0),
            Segment::Curve(5.0, 10.0, 25.0, 40.0, 25.0, 80.0),
            Segment::Close
        ]
    );
    // hvcurveto: start horizontal, end vertical.
    assert_eq!(
        path_of(&font, 4),
        [
            Segment::Move(0.0, 0.0),
            Segment::Curve(10.0, 0.0, 30.0, 30.0, 30.0, 70.0),
            Segment::Close
        ]
    );
    // vhcurveto with a final extra: ends with dxf = 50.
    assert_eq!(
        path_of(&font, 5),
        [
            Segment::Move(0.0, 0.0),
            Segment::Curve(0.0, 10.0, 20.0, 40.0, 60.0, 90.0),
            Segment::Close
        ]
    );
    assert_eq!(path_of(&font, 6).len(), 4);
    assert_eq!(path_of(&font, 7).len(), 4);
}

#[test]
fn hints_subroutines_flexes_and_arithmetic_of_type_2() {
    let hint = {
        // Two hstems, two vstems by hintmask, whose mask takes one byte.
        let mut c = cs(&[&[0, 10, 20, 30], &[5, 6]], &[&[18], &[]]);
        c.push(19);
        c.push(0xC0);
        c.extend(cs(&[&[0, 0], &[10, 0], &[]], &[RMOVETO, RLINETO, ENDCHAR]));
        c
    };
    // Subroutine bias 107: the number to call is 107 less.
    let calls = {
        let mut c = cs(&[&[0, 0]], &[RMOVETO]);
        c.extend(num(-107));
        c.push(10);
        c.extend(num(-107));
        c.push(29);
        c.push(14);
        c
    };
    let flex = cs(
        &[
            &[0, 0],
            &[10, 0, 10, 5, 10, 0, 10, 0, 10, -5, 10, 0, 50],
            &[],
        ],
        &[RMOVETO, &[12, 35], ENDCHAR],
    );
    let hflex = cs(
        &[&[0, 0], &[10, 10, 5, 10, 10, 10, 10], &[]],
        &[RMOVETO, &[12, 34], ENDCHAR],
    );
    let hflex1 = cs(
        &[&[0, 0], &[10, 5, 10, 5, 10, 10, 10, 5, 10], &[]],
        &[RMOVETO, &[12, 36], ENDCHAR],
    );
    let flex1 = cs(
        &[&[0, 0], &[10, 5, 10, 5, 10, 5, 10, 5, 10, 5, 20], &[]],
        &[RMOVETO, &[12, 37], ENDCHAR],
    );
    let arithmetic = {
        // 3 4 add -> 7; 10 2 sub -> 8; 3 3 mul -> 9; 20 4 div -> 5; neg; abs; dup; exch; drop; ...
        let mut c = cs(&[&[0, 0]], &[RMOVETO]);
        for (a, b, op) in [(3, 4, 10u8), (10, 2, 11), (3, 3, 24), (20, 4, 12)] {
            c.extend(num(a));
            c.extend(num(b));
            c.extend([12, op]);
        }
        // Stack: 7 8 9 5. Then neg abs, sqrt of 4, a dup, an exch, a drop, an index and a roll.
        c.extend([12, 14, 12, 9]);
        c.extend(num(16));
        c.extend([12, 26]);
        c.extend([12, 27, 12, 28, 12, 18]);
        c.extend(num(0));
        c.extend([12, 29]);
        c.extend(num(3));
        c.extend(num(1));
        c.extend([12, 30]);
        // Comparisons and conditionals.
        c.extend(num(1));
        c.extend(num(1));
        c.extend([12, 15, 12, 5]);
        c.extend(num(2));
        c.extend(num(3));
        c.extend([12, 3, 12, 4]);
        // A put and a get.
        c.extend(num(4));
        c.extend(num(0));
        c.extend([12, 20]);
        c.extend(num(0));
        c.extend([12, 21]);
        c.extend([12, 23]);
        c.extend(num(1));
        c.extend(num(2));
        c.extend(num(3));
        c.extend(num(4));
        c.extend([12, 22]);
        c.push(14);
        c
    };
    let seac = cs(&[&[30, 40, 65, 66]], &[ENDCHAR]);
    let font = cff(
        &[
            hint,
            calls,
            flex,
            hflex,
            hflex1,
            flex1,
            arithmetic,
            seac,
            cs(&[&[0, 0], &[10, 0], &[]], &[RMOVETO, RLINETO, ENDCHAR]),
            cs(&[&[5, 5], &[0, 10], &[]], &[RMOVETO, RLINETO, ENDCHAR]),
        ],
        // The local subroutine called by `-107 callsubr`: a line, then a return.
        &[cs(&[&[10, 0]], &[RLINETO])
            .into_iter()
            .chain([11])
            .collect()],
        &[cs(&[&[0, 10]], &[RLINETO])
            .into_iter()
            .chain([11])
            .collect()],
    );
    assert_eq!(
        path_of(&font, 1),
        [
            Segment::Move(0.0, 0.0),
            Segment::Line(10.0, 0.0),
            Segment::Close
        ]
    );
    assert_eq!(
        path_of(&font, 2),
        [
            Segment::Move(0.0, 0.0),
            Segment::Line(10.0, 0.0),
            Segment::Line(10.0, 10.0),
            Segment::Close
        ]
    );
    for glyph in 3..=7 {
        let path = path_of(&font, glyph);
        assert!(!path.is_empty(), "glyph {glyph}");
    }
    // flex is two curves.
    assert_eq!(path_of(&font, 3).len(), 4);
    // The accented glyph draws two glyphs: A as it is, B moved by (30, 40).
    let seac = path_of(&font, 8);
    assert!(seac.contains(&Segment::Move(0.0, 0.0)) && seac.contains(&Segment::Move(35.0, 45.0)));
}

#[test]
fn bad_cff_fonts_are_refused_and_their_charstrings_cannot_run_away() {
    assert!(Cff::parse(&[]).is_none());
    assert!(Cff::parse(b"not a font").is_none());
    assert!(Cff::parse(&[1, 0, 4, 1, 0, 0]).is_none());
    // A charstring that calls itself, and one that never ends a stack.
    let recursive = {
        let mut c = Vec::new();
        c.extend(num(-107));
        c.push(10);
        c
    };
    let font = cff(
        &[recursive, vec![0x0A; 40], cs(&[&[1; 100]], &[ENDCHAR])],
        &[num(-107).into_iter().chain([10]).collect()],
        &[],
    );
    let cff = Cff::parse(&font).unwrap();
    assert!(cff.outline(1, &|_| None).is_none());
    assert!(cff.outline(2, &|_| None).is_none());
    // A glyph that doesn't exist.
    assert!(cff.outline(99, &|_| None).is_none());
    assert!(cff.glyph_name(1).is_some());
    assert_eq!(cff.glyph_by_name(b"space"), Some(1));
    assert_eq!(cff.glyph_by_name(b"nothing"), None);
    assert!(cff.glyph_by_cid(1).is_none());
    assert!(cff.builtin_glyph(65).is_none());
}

// ---- Type 1 ----

fn encrypt(data: &[u8], key: u16, lead: usize) -> Vec<u8> {
    let mut r = key;
    let mut out = Vec::new();
    for &plain in std::iter::repeat_n(&0u8, lead).chain(data) {
        let cipher = plain ^ (r >> 8) as u8;
        r = u16::from(cipher)
            .wrapping_add(r)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        out.push(cipher);
    }
    out
}

fn t1num(v: i32) -> Vec<u8> {
    match v {
        -107..=107 => vec![(v + 139) as u8],
        108..=1131 => {
            let v = v - 108;
            vec![(v / 256 + 247) as u8, (v % 256) as u8]
        }
        -1131..=-108 => {
            let v = -v - 108;
            vec![(v / 256 + 251) as u8, (v % 256) as u8]
        }
        _ => {
            let mut out = vec![255];
            out.extend(v.to_be_bytes());
            out
        }
    }
}

fn t1(parts: &[(&[i32], &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (numbers, operator) in parts {
        for &n in *numbers {
            out.extend(t1num(n));
        }
        out.extend_from_slice(operator);
    }
    out
}

// A Type 1 font with `glyphs` and `subrs`, charstrings encrypted.
fn type1(glyphs: &[(&str, Vec<u8>)], subrs: &[Vec<u8>], hex: bool) -> Vec<u8> {
    let mut private = Vec::new();
    private.extend_from_slice(b"dup /Private 8 dict dup begin\n/RD {string currentfile exch readstring pop} executeonly def\n");
    private.extend_from_slice(format!("/lenIV 4 def\n/Subrs {} array\n", subrs.len()).as_bytes());
    for (i, subr) in subrs.iter().enumerate() {
        let body = encrypt(subr, 4330, 4);
        private.extend_from_slice(format!("dup {i} {} RD ", body.len()).as_bytes());
        private.extend(body);
        private.extend_from_slice(b" NP\n");
    }
    private.extend_from_slice(
        format!("ND\n2 index /CharStrings {} dict dup begin\n", glyphs.len()).as_bytes(),
    );
    for (name, code) in glyphs {
        let body = encrypt(code, 4330, 4);
        private.extend_from_slice(format!("/{name} {} RD ", body.len()).as_bytes());
        private.extend(body);
        private.extend_from_slice(b" ND\n");
    }
    private.extend_from_slice(b"end\nend\nreadonly put\n");
    let mut out = b"%!FontType1-1.0: Test\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\ncurrentdict end\ncurrentfile eexec\n".to_vec();
    let cipher = encrypt(&private, 55665, 4);
    if hex {
        out.extend(cipher.iter().flat_map(|b| format!("{b:02X}").into_bytes()));
        out.push(b'\n');
    } else {
        out.extend(cipher);
    }
    out.extend_from_slice(
        b"\n0000000000000000000000000000000000000000000000000000000000000000\ncleartomark\n",
    );
    out
}

fn t1path(font: &[u8], name: &str) -> Vec<Segment> {
    let font = Type1::parse(font).unwrap();
    font.outline(name.as_bytes(), &|code| match code {
        65 => Some("A"),
        66 => Some("B"),
        _ => None,
    })
    .unwrap()
    .segments
}

const HSBW: &[u8] = &[13];
const CLOSE: &[u8] = &[9];
const END: &[u8] = &[14];

#[test]
fn type_1_charstrings_draw_lines_curves_and_closed_paths() {
    let glyphs = vec![
        (
            "A",
            t1(&[
                (&[10, 500], HSBW),
                (&[100, 0], RMOVETO),
                (&[100, 0], RLINETO),
                (&[0, 100], RLINETO),
                (&[], CLOSE),
                (&[], END),
            ]),
        ),
        (
            "B",
            t1(&[
                (&[0, 500], HSBW),
                (&[0, 0], RMOVETO),
                (&[10], HLINETO),
                (&[20], VLINETO),
                (&[1, 2, 3, 4, 5, 6], RRCURVETO),
                (&[1, 2, 3, 4], &[30]),
                (&[1, 2, 3, 4], &[31]),
                (&[], CLOSE),
                (&[], END),
            ]),
        ),
        (
            "C",
            t1(&[
                (&[0, 500], HSBW),
                (&[0], &[22]),
                (&[10], &[4]),
                (&[300, 2], &[12, 12]),
                (&[], &[5][..]),
                (&[], END),
            ]),
        ),
        (".notdef", t1(&[(&[0, 500], HSBW), (&[], END)])),
    ];
    let font = type1(&glyphs, &[], false);
    assert_eq!(
        t1path(&font, "A"),
        [
            Segment::Move(110.0, 0.0),
            Segment::Line(210.0, 0.0),
            Segment::Line(210.0, 100.0),
            Segment::Close
        ]
    );
    let b = t1path(&font, "B");
    assert_eq!(b[1], Segment::Line(10.0, 0.0));
    assert_eq!(b[2], Segment::Line(10.0, 20.0));
    assert_eq!(b[3], Segment::Curve(11.0, 22.0, 14.0, 26.0, 19.0, 32.0));
    assert!(b.contains(&Segment::Curve(19.0, 33.0, 21.0, 36.0, 25.0, 36.0)));
    assert!(b.len() >= 6);
    assert!(t1path(&font, "C").len() >= 2);
    // The same font with its private part in hexadecimal.
    let hex = type1(&glyphs, &[], true);
    assert_eq!(t1path(&hex, "A"), t1path(&font, "A"));
    let parsed = Type1::parse(&font).unwrap();
    assert!(parsed.has_glyph(b"A") && !parsed.has_glyph(b"Z"));
    assert_eq!(parsed.glyph_names().count(), 4);
    assert_eq!(parsed.font_matrix[0], 0.001);
    assert!(parsed.outline(b"Z", &|_| None).is_none());
}

const RMOVETO_T1: &[u8] = &[21];
const RLINETO_T1: &[u8] = &[5];
const HLINETO_T1: &[u8] = &[6];
const VLINETO_T1: &[u8] = &[7];
const RRCURVETO_T1: &[u8] = &[8];

#[test]
fn type_1_subroutines_flexes_hint_replacement_and_accents() {
    // Subroutine 0 is the flex ending, 1 starts a flex, 2 adds a point, 3
    // does nothing, and 4 draws a line.
    let subrs = vec![
        t1(&[
            (&[3, 0], &[12, 16]),
            (&[], &[12, 17]),
            (&[], &[12, 17]),
            (&[], &[12, 33]),
            (&[], &[11]),
        ]),
        t1(&[(&[0, 1], &[12, 16]), (&[], &[11])]),
        t1(&[(&[0, 2], &[12, 16]), (&[], &[11])]),
        t1(&[(&[], &[11])]),
        t1(&[(&[7, 8], RLINETO_T1), (&[], &[11])]),
    ];
    let flex = {
        let mut c = t1(&[(&[0, 500], HSBW), (&[0, 0], RMOVETO_T1), (&[1], &[10])]);
        for (dx, dy) in [
            (50, 0),
            (-50, 0),
            (10, 5),
            (10, 0),
            (10, -5),
            (10, 0),
            (10, 5),
        ] {
            c.extend(t1(&[(&[dx, dy], RMOVETO_T1), (&[2], &[10])]));
        }
        c.extend(t1(&[(&[50, 80, 0], &[]), (&[0], &[10])]));
        c.extend(t1(&[(&[], END)]));
        c
    };
    let replacement = t1(&[
        (&[0, 500], HSBW),
        (&[0, 0], RMOVETO_T1),
        // 3 1 callothersubr pop callsubr: the hint replacement idiom, for subr 3.
        (&[3, 1, 3], &[12, 16]),
        (&[], &[12, 17]),
        (&[], &[10]),
        (&[4], &[10]),
        (&[], &[1]),
        (&[], &[3]),
        (&[], &[12, 0]),
        (&[], &[12, 1]),
        (&[], &[12, 2]),
        (&[], END),
    ]);
    let accented = t1(&[(&[0, 500], HSBW), (&[10, 20, 30, 65, 66], &[12, 6])]);
    let side = t1(&[
        (&[5, 500, 0, 0], &[12, 7]),
        (&[0, 0], RMOVETO_T1),
        (&[5, 5], RLINETO_T1),
        (&[], END),
    ]);
    let glyphs = vec![
        ("flex", flex),
        ("hints", replacement),
        ("accented", accented),
        (
            "A",
            t1(&[
                (&[10, 500], HSBW),
                (&[0, 0], RMOVETO_T1),
                (&[10, 0], RLINETO_T1),
                (&[], END),
            ]),
        ),
        (
            "B",
            t1(&[
                (&[10, 500], HSBW),
                (&[0, 0], RMOVETO_T1),
                (&[0, 10], RLINETO_T1),
                (&[], END),
            ]),
        ),
        ("side", side),
        ("bad", t1(&[(&[], &[10])])),
        ("empty", vec![]),
        ("long", t1(&[(&[1; 70], &[])])),
        ("big", t1(&[(&[100000, 3], &[]), (&[], END)])),
        (
            "hvcurve",
            t1(&[
                (&[0, 500], HSBW),
                (&[0, 0], RMOVETO_T1),
                (&[1, 2, 3, 4], &[31]),
                (&[], END),
            ]),
        ),
        (
            "lines",
            t1(&[
                (&[0, 500], HSBW),
                (&[0, 0], RMOVETO_T1),
                (&[5], HLINETO_T1),
                (&[6], VLINETO_T1),
                (&[1, 2, 3, 4, 5, 6], RRCURVETO_T1),
                (&[], END),
            ]),
        ),
    ];
    let font = type1(&glyphs, &subrs, false);
    let flexed = t1path(&font, "flex");
    // The flex is two curves, from the start through the reference point's
    // neighbors.
    assert_eq!(
        flexed
            .iter()
            .filter(|s| matches!(s, Segment::Curve(..)))
            .count(),
        2
    );
    let hints = t1path(&font, "hints");
    assert!(hints.contains(&Segment::Line(7.0, 8.0)));
    // The accent B is drawn at the base A, moved by its offset.
    let accented = t1path(&font, "accented");
    assert!(accented.contains(&Segment::Move(10.0, 0.0)));
    assert!(accented.iter().any(|s| matches!(s, Segment::Move(x, y) if (*x - 20.0).abs() < 1e-9 && (*y - 30.0).abs() < 1e-9)), "{accented:?}");
    assert!(!t1path(&font, "side").is_empty());
    let parsed = Type1::parse(&font).unwrap();
    for name in [&b"bad"[..], b"long"] {
        assert!(
            parsed.outline(name, &|_| None).is_none(),
            "{}",
            String::from_utf8_lossy(name)
        );
    }
    let _ = parsed.outline(b"big", &|_| None);
    assert!(!t1path(&font, "hvcurve").is_empty());
    assert!(!t1path(&font, "lines").is_empty());
}

#[test]
fn bad_type_1_fonts_are_refused() {
    assert!(Type1::parse(b"").is_none());
    assert!(Type1::parse(b"%!PS no eexec here").is_none());
    assert!(Type1::parse(b"%!FontType1\ncurrentfile eexec\n").is_none());
    // A PFB file: its segment headers are skipped.
    let glyphs = vec![(
        "A",
        t1(&[
            (&[0, 500], HSBW),
            (&[0, 0], RMOVETO_T1),
            (&[10, 0], RLINETO_T1),
            (&[], END),
        ]),
    )];
    let plain = type1(&glyphs, &[], false);
    let split = plain.iter().position(|&b| b == b'\n').unwrap() + 1;
    let mut pfb = Vec::new();
    for (kind, part) in [(1u8, &plain[..split]), (2u8, &plain[split..])] {
        pfb.extend([0x80, kind]);
        pfb.extend((part.len() as u32).to_le_bytes());
        pfb.extend_from_slice(part);
    }
    pfb.extend([0x80, 3]);
    assert!(Type1::parse(&pfb).is_none() || t1path(&pfb, "A").len() >= 2);
}

// ---- TrueType ----

#[test]
fn bad_truetype_fonts_are_refused_and_the_good_ones_map_their_characters() {
    assert!(TrueType::parse(&[]).is_none());
    assert!(TrueType::parse(b"OTTO").is_none());
    assert!(TrueType::parse(&[0, 1, 0, 0, 0, 0]).is_none());
    let data = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fonts/data/standard/OzaruStdSerif-Regular.ttf"),
    )
    .unwrap();
    let font = TrueType::parse(&data).unwrap();
    assert!(font.glyph_count() > 200 && font.units_per_em == 2048);
    let map = font.map(3, 1).unwrap();
    let a = map.glyph(u32::from('A')).unwrap();
    let outline = font.outline(a).unwrap();
    assert!(outline.segments.len() > 8);
    // The space has no contour, and a glyph that doesn't exist no outline.
    assert!(font.outline(map.glyph(32).unwrap()).unwrap().is_empty());
    assert!(font.outline(60000).is_none());
    assert!(font.glyph_by_name(b"A").is_none() || font.glyph_by_name(b"A") == Some(a));
    assert!(map.codes().len() > 100);
    // Accented letters are made of two glyphs.
    let e_acute = map.glyph(0xE9).unwrap();
    assert!(font.outline(e_acute).unwrap().segments.len() > outline.segments.len() / 2);
    // Every glyph of the font draws without trouble.
    for gid in 0..font.glyph_count() {
        let _ = font.outline(gid as u16);
    }
    // Damaged data is refused at the tables or at the glyphs.
    for cut in [100, 500, 2000, data.len() / 2] {
        if let Some(font) = TrueType::parse(&data[..cut]) {
            for gid in 0..font.glyph_count().min(50) {
                let _ = font.outline(gid as u16);
            }
        }
    }
    assert!(font.cff_table().is_none());
}
