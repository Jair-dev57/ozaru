use ozaru_engine::content::{Matrix, TextObject, TextRenderingMode, TextState};

#[test]
fn td_moves_from_the_start_of_the_line() {
    let mut text = TextObject::default();
    text.move_text(72.0, 700.0);
    text.advance(30.0, 0.0);
    // The second Td starts from the line, not from where the glyphs ended.
    text.move_text(0.0, -14.0);
    assert_eq!(text.matrix.apply(0.0, 0.0), (72.0, 686.0));
    assert_eq!(text.line_matrix, text.matrix);
}

#[test]
fn td_is_in_text_space() {
    let mut text = TextObject::default();
    text.set_matrix(Matrix::new(2.0, 0.0, 0.0, 2.0, 100.0, 100.0));
    text.move_text(10.0, 5.0);
    assert_eq!(text.matrix.apply(0.0, 0.0), (120.0, 110.0));
}

#[test]
fn next_line_moves_down_by_the_leading() {
    let mut text = TextObject::default();
    text.move_text(50.0, 500.0);
    text.next_line(12.0);
    text.next_line(12.0);
    assert_eq!(text.matrix.apply(0.0, 0.0), (50.0, 476.0));
}

#[test]
fn advance_moves_only_the_text_matrix() {
    let mut text = TextObject::default();
    text.set_matrix(Matrix::new(0.0, 1.0, -1.0, 0.0, 300.0, 100.0));
    text.advance(10.0, 0.0);
    // The text runs upwards on a rotated line.
    assert_eq!(text.matrix.apply(0.0, 0.0), (300.0, 110.0));
    assert_eq!(text.line_matrix.apply(0.0, 0.0), (300.0, 100.0));
}

#[test]
fn the_rendering_matrix_has_size_scaling_and_rise() {
    let mut text = TextObject::default();
    text.move_text(100.0, 200.0);
    let state = TextState {
        font_size: 10.0,
        horizontal_scaling: 0.5,
        rise: 3.0,
        ..TextState::default()
    };
    let ctm = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
    let matrix = text.rendering_matrix(&state, &ctm);
    assert_eq!(matrix, Matrix::new(10.0, 0.0, 0.0, 20.0, 200.0, 406.0));
}

#[test]
fn rendering_modes_are_numbered_from_0_to_7() {
    assert_eq!(
        TextRenderingMode::from_number(0),
        Some(TextRenderingMode::Fill)
    );
    assert_eq!(
        TextRenderingMode::from_number(3),
        Some(TextRenderingMode::Invisible)
    );
    assert_eq!(
        TextRenderingMode::from_number(7),
        Some(TextRenderingMode::Clip)
    );
    assert_eq!(TextRenderingMode::from_number(8), None);
    assert_eq!(TextRenderingMode::from_number(-1), None);
}

#[test]
fn the_default_text_state_is_the_standard_one() {
    let state = TextState::default();
    assert_eq!(state.horizontal_scaling, 1.0);
    assert_eq!(state.character_spacing, 0.0);
    assert!(state.font.is_none());
}
