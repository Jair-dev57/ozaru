mod common;

use common::{file_with, stream_object};
use ozaru_engine::base::{ErrorKind, Limits};
use ozaru_engine::functions::{Function, read_function};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::Resolver;

// Reads object 2 of a file whose objects are `objects`, as a function.
fn function(objects: &[(u32, String)]) -> Result<Function, ErrorKind> {
    let mut all = vec![(1, "<< /Type /Catalog >>".to_string())];
    all.extend_from_slice(objects);
    let file = file_with(&all);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    read_function(
        &mut resolver,
        &Object::Reference(ObjectId::new(2, 0)),
        Limits::default(),
    )
    .map_err(|error| error.kind())
}

fn calculator(program: &str, domain: &str, range: &str) -> Function {
    function(&[(
        2,
        stream_object(
            &format!("/FunctionType 4 /Domain [{domain}] /Range [{range}]"),
            program,
        ),
    )])
    .unwrap()
}

fn close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-9)
}

#[track_caller]
fn assert_close(a: Vec<f64>, b: &[f64]) {
    assert!(close(&a, b), "{a:?} != {b:?}");
}

#[test]
fn exponential_functions_go_from_c0_to_c1() {
    let linear = function(&[(
        2,
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 1] /C1 [1 0.5 0] /N 1 >>".to_string(),
    )])
    .unwrap();
    let squared = function(&[(2, "<< /FunctionType 2 /Domain [0 1] /N 2 >>".to_string())]).unwrap();

    assert_eq!((linear.inputs(), linear.outputs()), (1, 3));
    assert_close(linear.evaluate(&[0.5]), &[0.5, 0.25, 0.5]);
    assert_close(linear.evaluate(&[2.0]), &[1.0, 0.5, 0.0]);
    assert_close(squared.evaluate(&[0.5]), &[0.25]);
    assert_close(squared.evaluate(&[-1.0]), &[0.0]);
}

#[test]
fn stitching_functions_pick_the_function_of_each_part() {
    let stitched = function(&[
        (
            2,
            "<< /FunctionType 3 /Domain [0 1] /Functions [3 0 R 4 0 R] /Bounds [0.5] \
             /Encode [0 1 1 0] >>"
                .to_string(),
        ),
        (
            3,
            "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_string(),
        ),
        (
            4,
            "<< /FunctionType 2 /Domain [0 1] /C0 [10] /C1 [20] /N 1 >>".to_string(),
        ),
    ])
    .unwrap();

    assert_close(stitched.evaluate(&[0.25]), &[0.5]);
    // The second part is encoded backwards: 0.5 is 1 and 1 is 0.
    assert_close(stitched.evaluate(&[0.5]), &[20.0]);
    assert_close(stitched.evaluate(&[0.75]), &[15.0]);
    assert_close(stitched.evaluate(&[1.0]), &[10.0]);
}

#[test]
fn sampled_functions_interpolate_their_table() {
    // Three samples of 8 bits: 0, 51 (the byte of "3") and 0.
    let one = function(&[(
        2,
        stream_object(
            "/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [3] /BitsPerSample 8",
            "\u{0}3\u{0}",
        ),
    )])
    .unwrap();
    assert_close(one.evaluate(&[0.0]), &[0.0]);
    assert_close(one.evaluate(&[0.5]), &[51.0 / 255.0]);
    assert_close(one.evaluate(&[0.25]), &[25.5 / 255.0]);
    assert_close(one.evaluate(&[1.0]), &[0.0]);
}

#[test]
fn sampled_functions_of_two_inputs_interpolate_both_ways() {
    // A 2 x 2 table of 4 bits, the first input fastest: 0 15 / 15 0, the
    // bytes 0x0F and 0xF0, written by hand as they are above 127.
    let mut pdf = common::Pdf::new();
    let catalog = pdf.object(1, "<< /Type /Catalog >>");
    let table_offset = pdf.offset();
    pdf.write(
        "2 0 obj\n<< /FunctionType 0 /Domain [0 1 0 1] /Range [0 15] /Size [2 2] \
         /BitsPerSample 4 /Decode [0 15] /Length 2 >>\nstream\n",
    );
    pdf.file.extend_from_slice(&[0x0F, 0xF0]);
    pdf.write("\nendstream\nendobj\n");
    let in_use = |offset| ozaru_engine::xref::XrefEntry::InUse {
        offset,
        generation: 0,
    };
    let start = pdf.table(
        &[(1, in_use(catalog)), (2, in_use(table_offset))],
        "/Size 3 /Root 1 0 R",
    );
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();
    let table = read_function(
        &mut resolver,
        &Object::Reference(ObjectId::new(2, 0)),
        Limits::default(),
    )
    .unwrap();

    assert_close(table.evaluate(&[0.0, 0.0]), &[0.0]);
    assert_close(table.evaluate(&[1.0, 0.0]), &[15.0]);
    assert_close(table.evaluate(&[0.0, 1.0]), &[15.0]);
    assert_close(table.evaluate(&[1.0, 1.0]), &[0.0]);
    assert_close(table.evaluate(&[0.5, 0.5]), &[7.5]);
}

#[test]
fn calculator_programs_compute_their_outputs() {
    let inverse = calculator("{ 1 exch sub }", "0 1", "0 1");
    assert_close(inverse.evaluate(&[0.25]), &[0.75]);

    // The tint as CMYK: cyan and black from one input.
    let cmyk = calculator(
        "{ dup 0.5 mul exch 0 exch 0 exch 0.2 mul }",
        "0 1",
        "0 1 0 1 0 1 0 1",
    );
    assert_close(cmyk.evaluate(&[1.0]), &[0.5, 0.0, 0.0, 0.2]);

    let choice = calculator("{ 0.5 gt { 1 } { 0 } ifelse }", "0 1", "0 1");
    assert_close(choice.evaluate(&[0.75]), &[1.0]);
    assert_close(choice.evaluate(&[0.25]), &[0.0]);

    let only_if = calculator("{ dup 0.5 lt { pop 0 } if }", "0 1", "0 1");
    assert_close(only_if.evaluate(&[0.25]), &[0.0]);
    assert_close(only_if.evaluate(&[0.75]), &[0.75]);
}

#[test]
fn calculator_operators_follow_postscript() {
    let cases: &[(&str, &[f64])] = &[
        ("{ pop 7 2 idiv 7 2 mod }", &[3.0, 1.0]),
        ("{ pop -7.5 abs 2 sqrt dup mul }", &[7.5, 2.0]),
        (
            "{ pop 2.5 round -2.5 round 2.7 truncate 2.1 ceiling }",
            &[3.0, -2.0, 2.0, 3.0],
        ),
        ("{ pop 90 sin 0 cos 1 0 atan }", &[1.0, 1.0, 90.0]),
        ("{ pop 2 10 exp 100 log 1 ln }", &[1024.0, 2.0, 0.0]),
        ("{ pop 1 2 3 3 1 roll }", &[3.0, 1.0, 2.0]),
        ("{ pop 1 2 3 2 copy }", &[1.0, 2.0, 3.0, 2.0, 3.0]),
        ("{ pop 1 2 3 2 index }", &[1.0, 2.0, 3.0, 1.0]),
        (
            "{ pop 1 4 bitshift 16 -2 bitshift 6 3 and 6 3 or 6 3 xor }",
            &[16.0, 4.0, 2.0, 7.0, 5.0],
        ),
        (
            "{ pop true false or { 1 } { 0 } ifelse 3 3 eq { 1 } if 2 cvr 2.9 cvi }",
            &[1.0, 1.0, 2.0, 2.0],
        ),
    ];
    for (program, expected) in cases {
        let range = "-10000 10000 ".repeat(expected.len());
        let function = calculator(program, "0 1", &range);
        assert_close(function.evaluate(&[0.0]), expected);
    }
}

#[test]
fn calculator_errors_give_the_lowest_of_the_range() {
    for program in [
        "{ pop 1 0 div }",
        "{ pop }",
        "{ pop true }",
        "{ 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 }",
    ] {
        let function = calculator(program, "0 1", "0.2 1");
        assert_close(function.evaluate(&[0.5]), &[0.2]);
    }
}

#[test]
fn invalid_functions_are_errors() {
    let invalid = [
        "<< /FunctionType 2 /Domain [0 1] >>",
        "<< /FunctionType 5 /Domain [0 1] >>",
        "<< /FunctionType 2 /N 1 >>",
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0] /C1 [1] /N 1 >>",
        "<< /FunctionType 3 /Domain [0 1] /Functions [] /Encode [] >>",
        "<< /FunctionType 3 /Domain [0 1] /Functions [2 0 R] /Encode [0 1] >>",
        "[1 2 3]",
    ];
    for body in invalid {
        let kind = function(&[(2, body.to_string())]).unwrap_err();
        assert!(
            matches!(kind, ErrorKind::InvalidFunction),
            "{body}: {kind:?}"
        );
    }
    for program in [
        "1 2 add",
        "{ 1 2 frobnicate }",
        "{ { 1 } }",
        "{ 1 { 2 } { 3 } { 4 } ifelse",
        "{ ifelse }",
    ] {
        let kind = function(&[(
            2,
            stream_object("/FunctionType 4 /Domain [0 1] /Range [0 1]", program),
        )])
        .unwrap_err();
        assert_eq!(kind, ErrorKind::InvalidFunction, "{program}");
    }
}

#[test]
fn huge_tables_exceed_the_limits() {
    let kind = function(&[(
        2,
        stream_object(
            "/FunctionType 0 /Domain [0 1 0 1 0 1] /Range [0 1] /Size [100000 100000 100000] \
             /BitsPerSample 8",
            "",
        ),
    )])
    .unwrap_err();
    assert_eq!(
        kind,
        ErrorKind::LimitExceeded(ozaru_engine::base::Limit::DecodedLength)
    );
}
