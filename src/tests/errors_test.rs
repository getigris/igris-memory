use super::*;

#[test]
fn validation_error_has_correct_code() {
    let err = IgrisError::validation("title must not be empty");
    assert_eq!(err.code, ErrorCode::ValidationError);
    assert_eq!(err.error, "title must not be empty");
}

#[test]
fn not_found_has_correct_code() {
    let err = IgrisError::not_found("Observation 42 not found");
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn database_error_has_correct_code() {
    let err = IgrisError::database("constraint violation");
    assert_eq!(err.code, ErrorCode::DatabaseError);
}

#[test]
fn lock_error_has_correct_code() {
    let err = IgrisError::lock("mutex poisoned");
    assert_eq!(err.code, ErrorCode::LockError);
}

#[test]
fn error_serializes_to_json() {
    let err = IgrisError::validation("bad input");
    let json = err.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("should be valid JSON");
    assert_eq!(parsed["error"], "bad input");
    assert_eq!(parsed["code"], "VALIDATION_ERROR");
}

#[test]
fn not_found_serializes_to_json() {
    let err = IgrisError::not_found("missing");
    let json = err.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("should be valid JSON");
    assert_eq!(parsed["code"], "NOT_FOUND");
}

#[test]
fn display_shows_error_message() {
    let err = IgrisError::validation("bad");
    assert_eq!(format!("{err}"), "bad");
}

#[test]
fn from_string_creates_validation_error() {
    let err: IgrisError = "field is required".to_string().into();
    assert_eq!(err.code, ErrorCode::ValidationError);
}

#[test]
fn from_rusqlite_creates_database_error() {
    let sqlite_err = rusqlite::Error::QueryReturnedNoRows;
    let err: IgrisError = sqlite_err.into();
    assert_eq!(err.code, ErrorCode::DatabaseError);
}
