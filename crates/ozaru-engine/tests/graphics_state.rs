use ozaru_engine::content::{GraphicsStack, GraphicsState, Matrix};
use ozaru_engine::syntax::Object;

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn matrices_map_points() {
    let matrix = Matrix::new(2.0, 0.0, 0.0, 3.0, 10.0, 20.0);
    assert_eq!(matrix.apply(1.0, 1.0), (12.0, 23.0));
    assert_eq!(Matrix::IDENTITY.apply(4.0, 5.0), (4.0, 5.0));
    assert_eq!(Matrix::translation(1.0, 2.0).apply(0.0, 0.0), (1.0, 2.0));
}

#[test]
fn then_applies_the_first_matrix_first() {
    // Scale by 2, then move by (10, 0): (1, 1) goes to (12, 2).
    let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
    let shift = Matrix::translation(10.0, 0.0);
    assert_eq!(scale.then(&shift).apply(1.0, 1.0), (12.0, 2.0));
    // Move first, then scale: (1, 1) goes to (22, 2).
    assert_eq!(shift.then(&scale).apply(1.0, 1.0), (22.0, 2.0));
    // A rotation by 90 degrees and its inverse.
    let rotate = Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
    let back = Matrix::new(0.0, -1.0, 1.0, 0.0, 0.0, 0.0);
    assert!(close(rotate.then(&back).apply(3.0, 4.0), (3.0, 4.0)));
    assert!(close(rotate.apply(1.0, 0.0), (0.0, 1.0)));
}

#[test]
fn matrices_come_from_six_numbers() {
    let numbers: Vec<Object> = [1, 0, 0, 1]
        .map(Object::Integer)
        .into_iter()
        .chain([Object::Real(72.5), Object::Integer(-3)])
        .collect();
    assert_eq!(
        Matrix::from_objects(&numbers),
        Some(Matrix::new(1.0, 0.0, 0.0, 1.0, 72.5, -3.0))
    );
    assert_eq!(Matrix::from_objects(&numbers[..5]), None);
    let mut with_name = numbers.clone();
    with_name[2] = Object::Null;
    assert_eq!(Matrix::from_objects(&with_name), None);
    let mut huge = numbers;
    huge[0] = Object::Real(f64::INFINITY);
    assert_eq!(Matrix::from_objects(&huge), None);
}

#[test]
fn restore_goes_back_to_the_saved_state() {
    let mut stack = GraphicsStack::new(GraphicsState::default(), 10);
    assert!(stack.save());
    assert!(stack.transform(&Matrix::translation(5.0, 5.0)));
    assert!(stack.save());
    assert!(stack.transform(&Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0)));
    // cm applies before the current matrix: the scale happens first.
    assert_eq!(stack.current().ctm.apply(1.0, 1.0), (7.0, 7.0));
    assert_eq!(stack.depth(), 2);
    assert!(stack.restore());
    assert_eq!(stack.current().ctm, Matrix::translation(5.0, 5.0));
    assert!(stack.restore());
    assert_eq!(stack.current().ctm, Matrix::IDENTITY);
    // Nothing left to restore: the state is kept.
    assert!(!stack.restore());
    assert_eq!(stack.current().ctm, Matrix::IDENTITY);
}

#[test]
fn saves_past_the_limit_are_ignored_with_their_restores() {
    let mut stack = GraphicsStack::new(GraphicsState::default(), 1);
    assert!(stack.save());
    stack.transform(&Matrix::translation(1.0, 0.0));
    assert!(!stack.save());
    stack.transform(&Matrix::translation(1.0, 0.0));
    assert_eq!(stack.depth(), 2);
    // This restore matches the ignored save, so nothing changes.
    assert!(stack.restore());
    assert_eq!(stack.current().ctm, Matrix::translation(2.0, 0.0));
    assert!(stack.restore());
    assert_eq!(stack.current().ctm, Matrix::IDENTITY);
}

#[test]
fn transforms_that_overflow_are_refused() {
    let mut stack = GraphicsStack::new(GraphicsState::default(), 10);
    let huge = Matrix::new(1e200, 0.0, 0.0, 1e200, 0.0, 0.0);
    assert!(stack.transform(&huge));
    assert!(!stack.transform(&huge));
    assert_eq!(stack.current().ctm, huge);
}
