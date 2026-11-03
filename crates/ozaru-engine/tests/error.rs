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
    assert_eq!(
        Error::new(ErrorKind::UnexpectedToken, 0).to_string(),
        "unexpected token at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::InvalidDictionaryKey, 0).to_string(),
        "dictionary key is not a name at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::MissingDictionaryValue, 0).to_string(),
        "dictionary key has no value at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::InvalidReference, 0).to_string(),
        "invalid reference at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::InvalidObjectHeader, 0).to_string(),
        "invalid object header at byte 0"
    );
    assert_eq!(
        Error::new(ErrorKind::MissingEndstream, 0).to_string(),
        "missing endstream at byte 0"
    );
    let messages: Vec<String> = [
        ErrorKind::MissingStartxref,
        ErrorKind::InvalidStartxref,
        ErrorKind::NotAnXrefTable,
        ErrorKind::InvalidXrefSubsection,
        ErrorKind::InvalidXrefEntry,
        ErrorKind::MissingTrailer,
        ErrorKind::InvalidTrailer,
        ErrorKind::InvalidFlateData,
        ErrorKind::InvalidDecodeParameters,
        ErrorKind::UnsupportedPredictor,
        ErrorKind::InvalidPredictorData,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(
        messages,
        [
            "missing startxref",
            "invalid startxref offset",
            "not a cross-reference table",
            "invalid cross-reference subsection",
            "invalid cross-reference entry",
            "missing trailer",
            "trailer is not a dictionary",
            "invalid Flate data",
            "invalid decode parameters",
            "unsupported predictor",
            "invalid predictor data",
        ]
    );
}

#[test]
fn errors_work_with_the_standard_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(Error::new(ErrorKind::UnexpectedEnd, 0));

    assert_eq!(error.to_string(), "unexpected end of input at byte 0");
}
