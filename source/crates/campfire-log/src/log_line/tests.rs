use std::net::Ipv4Addr;

use tracing::info;

use serde::Serialize;

use super::*;
use crate::json_text::JsonText;
use crate::log_event::internals::round_trip;

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Opened {
    port: u16,
    #[serde(deserialize_with = "LogLine::text")]
    address: Ipv4Addr,
    #[serde(deserialize_with = "LogLine::json")]
    route: Route,
}

/// A value with no text form of its own, which an event logs as its JSON.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Route {
    Direct,
    Via { hops: u8 },
}

impl LogEvent for Opened {
    const MESSAGE: &'static str = "opened";

    fn log(&self) {
        info!(
            port = self.port,
            address = %self.address,
            route = %JsonText(&self.route),
            "{}",
            Self::MESSAGE
        );
    }
}

#[test]
fn a_line_reads_as_the_event_of_its_message_only() {
    round_trip(&Opened {
        port: 4433,
        address: Ipv4Addr::new(192, 168, 0, 4),
        route: Route::Via { hops: 2 },
    });
    let line = |fields: &str| {
        LogLine::parse(&format!(
            r#"{{"level":"INFO","target":"t","fields":{fields}}}"#
        ))
        .unwrap()
    };
    let other = line(r#"{"message":"closed","port":1}"#);
    assert_eq!((other.level, other.target.as_str()), (LogLevel::Info, "t"));
    assert!(other.read::<Opened>().is_none());
    // The message names the event, and its fields do not read: an address with a fifth part.
    let flawed = line(r#"{"message":"opened","port":1,"address":"1.2.3.4.5"}"#);
    assert!(matches!(flawed.read::<Opened>(), Some(Err(_))));
    assert!(LogLine::parse("{").is_err());
    assert!(LogLevel::Warn > LogLevel::Info && LogLevel::Error > LogLevel::Warn);
}
