use rmcp::model::LoggingLevel;

use super::level_rank;

#[test]
fn level_rank_orders_by_severity() {
    let levels = [
        LoggingLevel::Debug,
        LoggingLevel::Info,
        LoggingLevel::Notice,
        LoggingLevel::Warning,
        LoggingLevel::Error,
        LoggingLevel::Critical,
        LoggingLevel::Alert,
        LoggingLevel::Emergency,
    ];
    for pair in levels.windows(2) {
        assert!(
            level_rank(pair[0]) < level_rank(pair[1]),
            "{:?} should rank below {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn with_duration_inserts_field_on_object() {
    let data = serde_json::json!({ "id": 1 });
    let out = super::with_duration(data, 42);
    assert_eq!(out["id"], 1);
    assert_eq!(out["duration_ms"], 42);
}
