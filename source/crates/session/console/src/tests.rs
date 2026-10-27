use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use campfire_codec::{Binary, Json};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::*;

/// Asserts that `value` writes as `text` and reads back as itself, and decodes again from the
/// binary form a link carries it in.
fn round_trips<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T, text: &str) {
    assert_eq!(Json::write(value), text);
    assert_eq!(&Json::parse::<T>(text).unwrap(), value);
    assert_eq!(&Binary::decode::<T>(&Binary::encode(value)).unwrap(), value);
}

/// Asserts that `value` writes as `text` and reads back as itself, a line no link carries.
fn round_trips_as_text<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T, text: &str) {
    assert_eq!(Json::write(value), text);
    assert_eq!(&Json::parse::<T>(text).unwrap(), value);
}

#[test]
fn each_request_round_trips_through_json() {
    round_trips(
        &Request {
            id: RequestId(1),
            ask: ConsoleAsk::Status,
        },
        r#"{"id":1,"ask":"status"}"#,
    );
    let filters = [
        (UnitSelection::All, r#""all""#),
        (UnitSelection::PlayerSlot(2), r#"{"player_slot":2}"#),
        (UnitSelection::Team(1), r#"{"team":1}"#),
        (
            UnitSelection::Type("creep".to_owned()),
            r#"{"type":"creep"}"#,
        ),
        (UnitSelection::Ids(vec![9, 2]), r#"{"ids":[9,2]}"#),
        (
            UnitSelection::State(UnitActivity::Stuck),
            r#"{"state":"stuck"}"#,
        ),
    ];
    for (units, text) in filters {
        let request = Request {
            id: RequestId(u64::MAX),
            ask: ConsoleAsk::Units(UnitsAsk {
                units,
                detail: Detail::Concise,
                first: 40,
                limit: Some(20),
            }),
        };
        let line = format!(
            r#"{{"id":18446744073709551615,"ask":{{"units":{{"units":{text},"detail":"concise","first":40,"limit":20}}}}}}"#
        );
        round_trips(&request, &line);
    }
    let asks = [
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Tick(300),
                wall_seconds: None,
            }),
            r#"{"run":{"to":{"tick":300},"wall_seconds":null}}"#,
        ),
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Ticks(30),
                wall_seconds: Some(5),
            }),
            r#"{"run":{"to":{"ticks":30},"wall_seconds":5}}"#,
        ),
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Until(RunCondition::Unit {
                    id: 2,
                    state: UnitActivity::Idle,
                }),
                wall_seconds: None,
            }),
            r#"{"run":{"to":{"until":{"unit":{"id":2,"state":"idle"}}},"wall_seconds":null}}"#,
        ),
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Until(RunCondition::Event {
                    message: "a unit died".to_owned(),
                }),
                wall_seconds: None,
            }),
            r#"{"run":{"to":{"until":{"event":{"message":"a unit died"}}},"wall_seconds":null}}"#,
        ),
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Until(RunCondition::ScriptFailure),
                wall_seconds: None,
            }),
            r#"{"run":{"to":{"until":"script_failure"},"wall_seconds":null}}"#,
        ),
        (
            ConsoleAsk::Run(RunAsk {
                to: RunTo::Until(RunCondition::MatchEnd),
                wall_seconds: None,
            }),
            r#"{"run":{"to":{"until":"match_end"},"wall_seconds":null}}"#,
        ),
        (ConsoleAsk::Cancel(RequestId(4)), r#"{"cancel":4}"#),
        (
            ConsoleAsk::Pace(PaceAsk {
                paused: Some(false),
                speed: Some(GameSpeed::Half),
            }),
            r#"{"pace":{"paused":false,"speed":"half"}}"#,
        ),
        (
            ConsoleAsk::Pace(PaceAsk {
                paused: None,
                speed: None,
            }),
            r#"{"pace":{"paused":null,"speed":null}}"#,
        ),
    ];
    for (ask, text) in asks {
        let request = Request {
            id: RequestId(9),
            ask,
        };
        round_trips(&request, &format!(r#"{{"id":9,"ask":{text}}}"#));
    }
    // A run names its wall time, or takes two minutes.
    let wall = |wall_seconds| {
        RunAsk {
            to: RunTo::Tick(0),
            wall_seconds,
        }
        .wall()
    };
    assert_eq!(wall(None), Duration::from_secs(120));
    assert_eq!(wall(Some(5)), Duration::from_secs(5));

    // A table's first row is the first unless the request names another.
    let read: Request =
        Json::parse(r#"{"id":3,"ask":{"units":{"units":"all","detail":"detailed","limit":null}}}"#)
            .unwrap();
    let ConsoleAsk::Units(units) = read.ask else {
        panic!("{read:?}");
    };
    assert_eq!((units.first, units.limit), (0, None));
}

#[test]
fn each_line_round_trips_through_json() {
    // The fingerprint is one, whenever it is asked, and the opening's.
    let fingerprint = Output::fingerprint();
    assert_eq!(fingerprint, Output::fingerprint());
    let opening = Output::opening(GameRole::Server);
    let text = format!(
        r#"{{"opening":{{"role":"server","fingerprint":{}}}}}"#,
        u64::from_str_radix(&fingerprint.to_string(), 16).unwrap()
    );
    round_trips(&opening, &text);

    let status = SessionStatus {
        phase: SessionPhase::Running,
        rate: Some(30),
        state_hash: Some("ab".repeat(32)),
        pace: Some(PaceStatus {
            paused: true,
            speed: GameSpeed::Quadruple,
        }),
        players: vec![PlayerStatus {
            player_slot: 0,
            resources: BTreeMap::from([("gold".to_owned(), -5)]),
        }],
        script_failures: ScriptFailureCount {
            count: 2,
            latest: Some(FailedScriptCall {
                tick: 7,
                unit: Some(4),
                hook: "on_hit".to_owned(),
                error: "no such field".to_owned(),
            }),
        },
    };
    let reply = |answer| {
        Output::Reply(Reply {
            id: RequestId(5),
            tick: Some(299),
            answer,
        })
    };
    round_trips(
        &reply(ConsoleAnswer::Status(status)),
        &format!(
            r#"{{"reply":{{"id":5,"tick":299,"answer":{{"status":{{"phase":"running","rate":30,"state_hash":"{}","pace":{{"paused":true,"speed":"quadruple"}},"players":[{{"player_slot":0,"resources":{{"gold":-5}}}}],"script_failures":{{"count":2,"latest":{{"tick":7,"unit":4,"hook":"on_hit","error":"no such field"}}}}}}}}}}}}"#,
            "ab".repeat(32)
        ),
    );
    round_trips(
        &reply(ConsoleAnswer::Units(UnitTable {
            text: "tick 299\n".to_owned(),
            selected: 3,
            first: 1,
            shown: 2,
        })),
        r#"{"reply":{"id":5,"tick":299,"answer":{"units":{"text":"tick 299\n","selected":3,"first":1,"shown":2}}}}"#,
    );
    let ends = [
        (RunStop::Reached, "reached"),
        (RunStop::Held, "held"),
        (RunStop::WallTime, "wall_time"),
        (RunStop::Cancelled, "cancelled"),
        (RunStop::MatchEnded, "match_ended"),
    ];
    for (stop, text) in ends {
        round_trips(
            &reply(ConsoleAnswer::Ran(stop)),
            &format!(r#"{{"reply":{{"id":5,"tick":299,"answer":{{"ran":"{text}"}}}}}}"#),
        );
    }
    round_trips(
        &reply(ConsoleAnswer::Cancelled),
        r#"{"reply":{"id":5,"tick":299,"answer":"cancelled"}}"#,
    );
    round_trips(
        &reply(ConsoleAnswer::Paced(PaceStatus {
            paused: false,
            speed: GameSpeed::Normal,
        })),
        r#"{"reply":{"id":5,"tick":299,"answer":{"paced":{"paused":false,"speed":"normal"}}}}"#,
    );
    let refusals = [
        (
            ConsoleRefusal::Unread {
                error: "eof".to_owned(),
            },
            r#"{"unread":{"error":"eof"}}"#,
        ),
        (ConsoleRefusal::NoMatch, r#""no_match""#),
        (
            ConsoleRefusal::UnknownUnit { id: 3 },
            r#"{"unknown_unit":{"id":3}}"#,
        ),
        (ConsoleRefusal::NotLocal, r#""not_local""#),
        (
            ConsoleRefusal::RunUnderWay { run: RequestId(2) },
            r#"{"run_under_way":{"run":2}}"#,
        ),
        (
            ConsoleRefusal::UnknownType {
                name: "crep".to_owned(),
                nearest: NearestNames::of("crep", ["creep"]),
            },
            r#"{"unknown_type":{"name":"crep","nearest":["creep"]}}"#,
        ),
    ];
    for (refusal, text) in refusals {
        round_trips(
            &reply(ConsoleAnswer::Refused(refusal)),
            &format!(r#"{{"reply":{{"id":5,"tick":299,"answer":{{"refused":{text}}}}}}}"#),
        );
    }
    // Before the first tick, a reply names none.
    round_trips(
        &Reply {
            id: RequestId(0),
            tick: None,
            answer: ConsoleAnswer::Refused(ConsoleRefusal::NoMatch),
        },
        r#"{"id":0,"tick":null,"answer":{"refused":"no_match"}}"#,
    );

    let event = Output::Event(ConsoleEvent {
        level: EventLevel::Warn,
        target: "campfire_runner".to_owned(),
        tick: Some(7),
        message: Some("a script call failed".to_owned()),
        fields: BTreeMap::from([
            ("a".to_owned(), FieldValue::Bool(true)),
            ("b".to_owned(), FieldValue::Integer(-1)),
            ("c".to_owned(), FieldValue::Unsigned(u64::MAX)),
            ("d".to_owned(), FieldValue::Float(0.5)),
            ("e".to_owned(), FieldValue::Text("x".to_owned())),
        ]),
    });
    round_trips_as_text(
        &event,
        r#"{"event":{"level":"warn","target":"campfire_runner","tick":7,"message":"a script call failed","fields":{"a":true,"b":-1,"c":18446744073709551615,"d":0.5,"e":"x"}}}"#,
    );
}
