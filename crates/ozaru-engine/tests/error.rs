use ozaru_engine::base::{Error, ErrorKind, Limit};

#[test]
fn errors_keep_their_kind_and_offset() {
    let error = Error::new(ErrorKind::UnexpectedEnd, 42);

    assert_eq!(error.kind(), ErrorKind::UnexpectedEnd);
    assert_eq!(error.offset(), 42);
}

#[test]
fn errors_describe_what_happened_and_where() {
    assert_eq!(
        Error::new(ErrorKind::UnexpectedEnd, 7).to_string(),
        "unexpected end of input at byte 7"
    );
    assert_eq!(
        Error::new(ErrorKind::LimitExceeded(Limit::Depth), 120).to_string(),
        "nesting depth limit exceeded at byte 120"
    );
    assert_eq!(
        Error::new(ErrorKind::UnexpectedByte(b')'), 3).to_string(),
        "unexpected byte 0x29 at byte 3"
    );
    assert_eq!(
        Error::new(ErrorKind::InvalidNumber, 0).to_string(),
        "invalid number at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::NumberOutOfRange, 0).to_string(),
        "number out of range at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::InvalidHexString, 0).to_string(),
        "invalid hexadecimal string at byte 0"
    );
}

#[test]
fn errors_work_with_the_standard_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(Error::new(ErrorKind::UnexpectedEnd, 0));

    assert_eq!(error.to_string(), "unexpected end of input at byte 0");
}
