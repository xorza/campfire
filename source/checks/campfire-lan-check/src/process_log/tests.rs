use campfire_common::{PlayerSlot, Tick};
use campfire_net::MatchStarted;

use super::*;

#[test]
fn a_log_drops_its_unfinished_last_line_and_names_a_line_it_cannot_read() {
    let started = r#"{"level":"INFO","target":"t","fields":{"message":"the match started","slot":1,"start_tick":5}}"#;
    let text = format!("{started}\n{}", &started[..20]);
    let log = ProcessLog::parse(Process::Bot(0), &text).unwrap();
    assert_eq!(log.lines().len(), 1);
    assert_eq!(
        log.read_all::<MatchStarted>().unwrap(),
        [MatchStarted {
            slot: PlayerSlot::new(1),
            start_tick: 5
        }]
    );
    // The event of the second line has a field that does not read.
    let flawed = started.replace(r#""slot":1"#, r#""slot":"one""#);
    let log = ProcessLog::parse(Process::Bot(0), &format!("{started}\n{flawed}\n")).unwrap();
    // The first event reads, whatever follows it.
    assert_eq!(
        log.first::<MatchStarted>()
            .unwrap()
            .map(|started| started.slot),
        Some(PlayerSlot::new(1))
    );
    assert!(
        ProcessLog::empty(Process::Server)
            .first::<MatchStarted>()
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        log.read_all::<MatchStarted>(),
        Err(CheckError::Event {
            process: Process::Bot(0),
            line: 2,
            ..
        })
    ));
    // A line that is not JSON fails the parse, by its number.
    assert!(matches!(
        ProcessLog::parse(Process::Server, &format!("{started}\n{{\n")),
        Err(CheckError::Event {
            process: Process::Server,
            line: 2,
            ..
        })
    ));
    assert!(
        ProcessLog::parse(Process::Server, "")
            .unwrap()
            .lines()
            .is_empty()
    );
}

#[test]
fn a_bot_keeps_the_orders_it_sent_but_the_last_each_resume_discarded() {
    let sent = |stamp: u64, orders: usize| {
        format!(
            r#"{{"level":"DEBUG","target":"t","fields":{{"message":"{}","stamp":{stamp},"orders":{orders}}}}}"#,
            OrdersSent::MESSAGE
        )
    };
    let discarded = |count: u64| {
        format!(
            r#"{{"level":"WARN","target":"t","fields":{{"message":"{}","slot":0,"count":{count}}}}}"#,
            InputsDiscarded::MESSAGE
        )
    };
    // 2 orders at 20 and 1 at 50, then a resume that drops 2 inputs: the order at 50 and the
    // second at 20. Then 1 at 60, and a resume that drops 5, more than it holds: none stays.
    let lines = [sent(20, 2), sent(50, 1), discarded(2), sent(60, 1)];
    let log = ProcessLog::parse(Process::Bot(0), &format!("{}\n", lines.join("\n"))).unwrap();
    let kept = |stamp, orders| OrdersSent {
        stamp: Tick::new(stamp),
        orders,
    };
    assert_eq!(log.kept_orders().unwrap(), [kept(20, 1), kept(60, 1)]);
    let all = [lines.join("\n"), discarded(5)].join("\n");
    let log = ProcessLog::parse(Process::Bot(0), &format!("{all}\n")).unwrap();
    assert_eq!(log.kept_orders().unwrap(), []);
}
