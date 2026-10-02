use std::net::Ipv4Addr;

use tracing::info;

use super::*;
use crate::log_event::internals::round_trip;

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Opened {
    port: u16,
    #[serde(deserialize_with = "LogLine::text")]
    address: Ipv4Addr,
}

impl LogEvent for Opened {
    const MESSAGE: &'static str = "opened";

    fn log(&self) {
        info!(port = self.port, address = %self.address, "{}", Self::MESSAGE);
    }
}

#[test]
fn a_line_reads_as_the_event_of_its_message_only() {
    round_trip(&Opened {
        port: 4433,
        address: Ipv4Addr::new(192, 168, 0, 4),
    });
    let line = |fields: &str| {
        LogLine::parse(&format!(
            r#"{{"level":"INFO","target":"t","fields":{fields}}}"#
        ))
        .unwrap()
    };
    let other = line(r#"{"message":"closed","port":1}"#);
    assert_eq!((other.level, other.target.as_str()), (Level::Info, "t"));
    assert!(other.read::<Opened>().is_none());
    // The message names the event, and its fields do not read: an address with a fifth part.
    let flawed = line(r#"{"message":"opened","port":1,"address":"1.2.3.4.5"}"#);
    assert!(matches!(flawed.read::<Opened>(), Some(Err(_))));
    assert!(LogLine::parse("{").is_err());
    assert!(Level::Warn > Level::Info && Level::Error > Level::Warn);
}
