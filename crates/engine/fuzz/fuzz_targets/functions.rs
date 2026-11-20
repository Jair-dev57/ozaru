#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::functions::read_function;
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::Resolver;

// The input is the program of a calculator function and the samples of a
// table, in a small PDF. Reading either never panics, and evaluating gives
// as many outputs as the function says, all finite.
fuzz_target!(|data: &[u8]| {
    let Some((&shape, data)) = data.split_first() else {
        return;
    };
    let size = 1 + usize::from(shape & 0x0F);
    let bits = [1, 2, 4, 8, 12, 16, 24, 32][usize::from(shape >> 4) % 8];
    let functions = [
        "/FunctionType 4 /Domain [0 1 -5 5] /Range [0 1 0 1 0 1]".to_string(),
        format!(
            "/FunctionType 0 /Domain [0 1 0 1] /Range [0 1 -1 1] /Size [{size} 3] \
             /BitsPerSample {bits}"
        ),
    ];
    for entries in functions {
        let mut file = b"%PDF-1.7\n".to_vec();
        let catalog = file.len();
        file.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        let function = file.len();
        file.extend_from_slice(
            format!("2 0 obj\n<< {entries} /Length {} >>\nstream\n", data.len()).as_bytes(),
        );
        file.extend_from_slice(data);
        file.extend_from_slice(b"\nendstream\nendobj\n");
        let xref = file.len();
        file.extend_from_slice(
            format!(
                "xref\n0 3\n0000000000 65535 f \n{catalog:010} 00000 n \n{function:010} 00000 n \n\
                 trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
            )
            .as_bytes(),
        );
        let Ok(mut resolver) = Resolver::open(&file, Limits::default()) else {
            continue;
        };
        let Ok(function) = read_function(
            &mut resolver,
            &Object::Reference(ObjectId::new(2, 0)),
            Limits::default(),
        ) else {
            continue;
        };
        for input in [[0.0, 0.0], [0.5, -2.0], [1.0, 5.0], [0.25, 0.75], [-3.0, 9.0]] {
            let out = function.evaluate(&input);
            assert_eq!(out.len(), function.outputs());
            assert!(out.iter().all(|v| v.is_finite()), "{out:?}");
        }
    }
});
