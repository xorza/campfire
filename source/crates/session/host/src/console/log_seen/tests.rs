use campfire_log::{LogLevel, LogValue};

use super::*;

#[test]
fn it_counts_the_failed_calls_and_sees_the_event_it_waits_for() {
    let text = |text: &str| LogValue::Text(text.to_owned());
    let failed = LogRecord {
        level: LogLevel::Warn,
        target: "campfire_runner",
        fields: vec![
            ("message", text("a script call failed and changed nothing")),
            ("tick", LogValue::Unsigned(7)),
            ("unit", LogValue::Unsigned(4)),
            ("hook", text("on_hit")),
            ("error", text("no")),
        ],
    };
    let died = LogRecord {
        level: LogLevel::Info,
        target: "campfire_host",
        fields: vec![("message", text("a unit died"))],
    };
    let mut seen = LogSeen::default();
    // An event before the watch is not seen; the watch sees the next.
    seen.see(&died);
    seen.watch("a unit died".to_owned());
    assert!(!seen.is_watched_seen());
    seen.see(&failed);
    assert!(!seen.is_watched_seen());
    seen.see(&died);
    assert!(seen.is_watched_seen());
    seen.unwatch();
    assert!(!seen.is_watched_seen());
    assert_eq!(
        seen.failures(),
        &ScriptFailureCount {
            count: 1,
            latest: Some(FailedScriptCall {
                tick: 7,
                unit: Some(4),
                hook: "on_hit".to_owned(),
                error: "no".to_owned(),
            }),
        }
    );
}
