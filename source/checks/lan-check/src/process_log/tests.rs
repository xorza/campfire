use campfire_math::PlayerSlot;
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
