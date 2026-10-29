use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tracing::subscriber;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

/// The records `log` makes, as a layer takes them.
fn records(log: impl FnOnce()) -> Vec<LogRecord> {
    let records = Arc::new(Mutex::new(Vec::new()));
    let taken = Arc::clone(&records);
    let layer = RecordLayer::new(move |record: &LogRecord| {
        taken.lock().unwrap().push(record.clone());
    });
    subscriber::with_default(tracing_subscriber::registry().with(layer), log);
    Arc::try_unwrap(records).unwrap().into_inner().unwrap()
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
enum Side {
    North,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Fields {
    flag: bool,
    count: u8,
    delta: i32,
    share: f64,
    side: Side,
    name: String,
    lost: Option<u32>,
}

#[test]
fn a_record_holds_each_field_typed_and_reads_as_json_would() {
    // Each kind of field, `?` and `%` as text, in the order the event records them, the message
    // first; and read as a struct, a unit variant by its name and an absent field as none.
    let [record] = records(|| {
        tracing::warn!(
            flag = true,
            count = 3_u64,
            delta = -2_i64,
            share = 0.5,
            side = ?Side::North,
            name = %"rime",
            "seen"
        );
    })
    .try_into()
    .unwrap();
    assert_eq!(
        (record.level, record.target),
        (LogLevel::Warn, module_path!())
    );
    assert_eq!(
        record.fields,
        [
            ("message", LogValue::Text("seen".to_owned())),
            ("flag", LogValue::Bool(true)),
            ("count", LogValue::Unsigned(3)),
            ("delta", LogValue::Integer(-2)),
            ("share", LogValue::Float(0.5)),
            ("side", LogValue::Text("North".to_owned())),
            ("name", LogValue::Text("rime".to_owned())),
        ]
    );
    assert_eq!(record.text("message"), Some("seen"));
    assert_eq!(record.text("count"), None);
    let read: Fields = record.read_fields().unwrap();
    assert_eq!(
        read,
        Fields {
            flag: true,
            count: 3,
            delta: -2,
            share: 0.5,
            side: Side::North,
            name: "rime".to_owned(),
            lost: None,
        }
    );
    // A count past its width does not read.
    let [past] = records(|| tracing::info!(count = 300_u64, "seen"))
        .try_into()
        .unwrap();
    assert!(past.read_fields::<Fields>().is_err());
}
