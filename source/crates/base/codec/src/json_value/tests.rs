use std::collections::BTreeMap;

use super::*;
use crate::json::Json;

#[test]
fn a_value_gives_its_members_text_and_reads_again_as_a_type() {
    #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
    struct Event {
        message: String,
        count: u8,
    }
    let value: JsonValue = Json::parse(r#"{"message":"hi","count":3}"#).unwrap();
    assert_eq!(value.text("message"), Some("hi"));
    assert_eq!(value.text("count"), None);
    assert_eq!(value.text("none"), None);
    assert_eq!(value.to_string(), r#"{"count":3,"message":"hi"}"#);
    let event = Event {
        message: "hi".to_owned(),
        count: 3,
    };
    assert_eq!(value.read::<Event>().unwrap(), event);
    assert!(value.read::<BTreeMap<String, String>>().is_err());
}
