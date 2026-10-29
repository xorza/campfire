use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;

use campfire_log::LogValue;
use tracing::subscriber;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

#[test]
fn a_tap_gives_each_record_once_it_starts() {
    let tap = LogTap::default();
    let records = Arc::new(Mutex::new(Vec::<LogRecord>::new()));
    let layer = tap.layer();
    subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        tracing::info!(n = 1, "before");
        let taken = Arc::clone(&records);
        tap.start(move |record| taken.lock().unwrap().push(record.clone()));
        tracing::info!(n = 2, "after");
    });
    let records = records.lock().unwrap();
    let [record] = records.as_slice() else {
        panic!("{records:?}");
    };
    assert_eq!(record.text("message"), Some("after"));
    assert!(record.fields.contains(&("n", LogValue::Integer(2))));
    assert!(tap.is_started());
    let again = panic::catch_unwind(AssertUnwindSafe(|| tap.start(|_| {})));
    assert!(again.is_err());
}
