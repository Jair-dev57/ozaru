//! The colors of shadings at points of their space.

use ozaru_engine::Document;
use ozaru_engine::render::{Shading, read_shading};
use ozaru_engine::syntax::{Object, ObjectId};

mod common;
use common::{file_with, stream_object};

// The shadings written as the bodies of objects 5 and up.
fn shadings(bodies: &[String]) -> Vec<Option<Shading>> {
    let mut objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>".to_string(),
        ),
        (
            4,
            stream_object(
                "/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]",
                "{ 0 }",
            ),
        ),
    ];
    for (i, body) in bodies.iter().enumerate() {
        objects.push((5 + i as u32, body.clone()));
    }
    let pdf = file_with(&objects);
    let mut document = Document::open(&pdf).unwrap();
    let limits = document.limits();
    (0..bodies.len())
        .map(|i| {
            let id = ObjectId {
                number: 5 + i as u32,
                generation: 0,
            };
            read_shading(document.resolver(), &Object::Reference(id), limits)
        })
        .collect()
}

const RAMP: &str = "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >>";

#[test]
fn an_axial_shading_blends_along_its_line_and_extends_if_asked() {
    let all = shadings(&[
        format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 10 0] /Function {RAMP} >>"),
        format!(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 10 0] /Function {RAMP} /Extend [true false] >>"
        ),
        format!(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 10 0] /Function {RAMP} /BBox [0 0 10 5] \
                 /Background [1 0 0] >>"
        ),
        format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [3 3 3 3] /Function {RAMP} >>"),
    ]);
    let plain = all[0].as_ref().unwrap();
    assert_eq!(plain.color_at(0.0, 5.0), Some([0, 0, 0]));
    assert_eq!(plain.color_at(10.0, 5.0), Some([255, 255, 255]));
    let middle = plain.color_at(5.0, 100.0).unwrap();
    assert!((i32::from(middle[0]) - 128).abs() <= 1);
    assert_eq!(plain.color_at(-1.0, 0.0), None);
    assert_eq!(plain.color_at(11.0, 0.0), None);
    let extended = all[1].as_ref().unwrap();
    assert_eq!(extended.color_at(-50.0, 0.0), Some([0, 0, 0]));
    assert_eq!(extended.color_at(50.0, 0.0), None);
    // The box cuts, and the background is for patterns to use.
    let boxed = all[2].as_ref().unwrap();
    assert!(boxed.color_at(5.0, 2.0).is_some() && boxed.color_at(5.0, 6.0).is_none());
    assert_eq!(boxed.background, Some([255, 0, 0]));
    // A line of no length paints nothing.
    assert_eq!(all[3].as_ref().unwrap().color_at(3.0, 3.0), None);
}

#[test]
fn a_radial_shading_blends_between_two_circles() {
    let all = shadings(&[
        format!(
            "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [5 5 0 5 5 4] /Function {RAMP} >>"
        ),
        format!(
            "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [2 5 1 8 5 1] /Function {RAMP} /Extend [true true] >>"
        ),
        format!(
            "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [0 0 1 10 0 3] /Function {RAMP} /Extend [false true] >>"
        ),
    ]);
    let circles = all[0].as_ref().unwrap();
    assert_eq!(circles.color_at(5.0, 5.0), Some([0, 0, 0]));
    assert_eq!(circles.color_at(9.0, 5.0), Some([255, 255, 255]));
    assert_eq!(circles.color_at(5.0, 12.0), None);
    // Circles of the same radius make a band, which extends to both sides.
    let band = all[1].as_ref().unwrap();
    assert!(band.color_at(5.0, 5.0).is_some());
    assert!(band.color_at(5.0, 7.0).is_none());
    assert!(band.color_at(-5.0, 5.0).is_some());
    // A cone: it opens to the right and is cut on the left.
    let cone = all[2].as_ref().unwrap();
    assert!(cone.color_at(20.0, 0.0).is_some());
    assert!(cone.color_at(-5.0, 0.0).is_none());
}

#[test]
fn a_function_of_the_position_and_functions_in_an_array() {
    let all = shadings(&[
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Domain [0 1 0 1] /Matrix [10 0 0 10 0 0] /Function 4 0 R >>".to_string(),
        format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] /Function [{RAMP}] >>"),
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] /Function \
         [<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >>] >>"
            .to_string(),
    ]);
    // The PostScript function leaves 0 for red and gives the position... as it is: black here.
    let by_position = all[0].as_ref().unwrap();
    assert!(by_position.color_at(5.0, 5.0).is_some());
    assert_eq!(by_position.color_at(15.0, 5.0), None);
    // A function with three outputs, wrongly given as a one-element array of a
    // one-output function, still gives colors.
    assert!(all[1].as_ref().unwrap().color_at(0.5, 0.0).is_some());
    let mixed = all[2].as_ref().unwrap();
    assert_eq!(mixed.color_at(0.0, 0.0), Some([0, 255, 0]));
    assert_eq!(mixed.color_at(1.0, 0.0), Some([255, 0, 0]));
}

#[test]
fn shadings_that_cannot_be_read_are_none() {
    let all = shadings(&[
        // A mesh, which isn't read yet.
        "<< /ShadingType 4 /ColorSpace /DeviceRGB >>".to_string(),
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1] /Function << /FunctionType 2 /Domain [0 1] /N 1 >> >>".to_string(),
        "<< /ShadingType 2 /Coords [0 0 1 0] >>".to_string(),
        "<< /ShadingType 2 /ColorSpace /Pattern /Coords [0 0 1 0] >>".to_string(),
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] >>".to_string(),
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [0 0 0 0 0 0] /Function 4 0 R >>".to_string(),
        "42".to_string(),
    ]);
    for (i, shading) in all.iter().enumerate() {
        assert!(shading.is_none(), "{i}");
    }
}
