//! The server's console (console.md, A game's console is its standard streams): started with
//! `--console stdio`, it opens with its fingerprint, answers each request with its id and the
//! tick it read, sends its log's events between, and stops as its input ends, as a stop's signal
//! stops it.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdout, Command, Stdio};

use campfire_codec::Json;
use campfire_common::ExitStatus;
use campfire_console::{
    ConsoleAnswer, ConsoleAsk, ConsoleRefusal, Detail, GameRole, Output, Reply, Request, RequestId,
    SessionPhase, UnitSelection, UnitsAsk,
};
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;

/// The next line of `out` that decodes as a console's line; an event's is skipped.
fn next_reply(out: &mut BufReader<ChildStdout>) -> Reply {
    loop {
        let mut line = String::new();
        let read = out.read_line(&mut line).unwrap();
        assert!(read > 0, "the server's output ended before its reply");
        match Json::parse::<Output>(&line) {
            Ok(Output::Reply(reply)) => return reply,
            Ok(Output::Event(_)) => {}
            read => panic!("{line}: {read:?}"),
        }
    }
}

#[test]
fn a_console_answers_over_the_pipes_and_its_end_stops_the_server() {
    let test_dir = TestDir::new();
    let mut server = Command::new(env!("CARGO_BIN_EXE_campfire-server"))
        .env("RUST_LOG", "warn")
        .env_remove("CAMPFIRE_LOG")
        .env_remove("CAMPFIRE_LOG_FILTER")
        .env_remove("CAMPFIRE_LOG_FILTER_EXTRA")
        .arg("--data")
        .arg(test_dir.path("data"))
        .arg("--no-port-map")
        .arg("--console")
        .arg("stdio")
        .arg(TestMode::Lane.dir())
        .arg("127.0.0.1:0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(server.stdout.take().unwrap());
    let mut input = server.stdin.take().unwrap();

    // Its first line, before its load: the server's role and this build's fingerprint.
    let mut first = String::new();
    out.read_line(&mut first).unwrap();
    assert_eq!(
        Json::parse::<Output>(&first).unwrap(),
        Output::opening(GameRole::Server)
    );

    // Two requests at once, answered in their order, each with its id and the tick it read, none
    // in the lobby: the lobby's status, and a table, which no match yet holds.
    let units = ConsoleAsk::Units(UnitsAsk {
        units: UnitSelection::All,
        detail: Detail::Concise,
        first: 0,
        limit: None,
    });
    for (id, ask) in [(7, ConsoleAsk::Status), (8, units)] {
        let request = Request {
            id: RequestId(id),
            ask,
        };
        writeln!(input, "{}", Json::write(&request)).unwrap();
    }
    input.flush().unwrap();
    let status = next_reply(&mut out);
    assert_eq!((status.id, status.tick), (RequestId(7), None));
    let ConsoleAnswer::Status(status) = status.answer else {
        panic!("{status:?}");
    };
    assert_eq!(status.phase, SessionPhase::Lobby);
    assert_eq!(
        next_reply(&mut out),
        Reply {
            id: RequestId(8),
            tick: None,
            answer: ConsoleAnswer::Refused(ConsoleRefusal::NoMatch),
        }
    );

    // The input's end stops it, as a stop's signal does.
    drop(input);
    let status = server.wait().unwrap();
    assert_eq!(
        status.code(),
        Some(i32::from(ExitStatus::Interrupted.code()))
    );
}
