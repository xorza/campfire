use bevy_state::state::{NextState, State};
use campfire_common::Tick;
use campfire_console::{FailedScriptCall, ScriptFailureCount};

use super::*;

/// A server in its lobby with no match, its console's pace `pace`, its tap having seen a call
/// fail.
fn lobby(pace: Option<Arc<Pace>>) -> World {
    let mut world = World::new();
    world.insert_resource(State::new(ServerPhase::Lobby));
    world.init_resource::<NextState<ServerPhase>>();
    world.init_resource::<Time<Real>>();
    world.init_resource::<Time<Fixed>>();
    let seen = Arc::new(Mutex::new(LogSeen::default()));
    let failed = r#"{"level":"WARN","target":"campfire_runner","fields":{"message":"a script call failed and changed nothing","tick":3,"hook":"on_think","error":"no"}}"#;
    seen.lock()
        .unwrap()
        .see(&campfire_log::LogLine::parse(failed).unwrap());
    world.insert_resource(MatchConsole::new(pace, seen));
    world
}

/// The replies of `world`'s console to `asks`, answered in their order, ids from 1.
fn answers(world: &World, asks: impl IntoIterator<Item = ConsoleAsk>) -> Vec<Reply> {
    let console = world.resource::<MatchConsole>();
    console.push(asks.into_iter().zip(1..).map(|(ask, id)| Request {
        id: RequestId(id),
        ask,
    }));
    MatchConsole::answer(world);
    let mut replies = Vec::new();
    world.resource::<MatchConsole>().take_replies(&mut replies);
    replies
}

fn run(to: RunTo, wall_seconds: Option<u32>) -> ConsoleAsk {
    ConsoleAsk::Run(RunAsk { to, wall_seconds })
}

/// The answers of `replies`, in their order.
fn answered(replies: Vec<Reply>) -> Vec<ConsoleAnswer> {
    replies.into_iter().map(|reply| reply.answer).collect()
}

#[test]
fn a_console_answers_its_requests_and_refuses_what_it_cannot() {
    let pace = Arc::new(Pace::default());
    pace.set_paused(true);
    pace.set_speed(PaceSpeed::Double);
    let world = lobby(Some(Arc::clone(&pace)));
    let units = ConsoleAsk::Units(UnitsAsk {
        units: UnitSelection::All,
        detail: Detail::Concise,
        first: 0,
        limit: None,
    });
    let replies = answers(
        &world,
        [
            ConsoleAsk::Status,
            units,
            run(RunTo::Tick(5), None),
            ConsoleAsk::Cancel(RequestId(9)),
            ConsoleAsk::Pace(PaceAsk {
                paused: Some(false),
                speed: None,
            }),
        ],
    );
    let ids: Vec<(RequestId, Option<u64>)> = (replies.iter())
        .map(|reply| (reply.id, reply.tick))
        .collect();
    assert_eq!(
        ids,
        (1..=5).map(|id| (RequestId(id), None)).collect::<Vec<_>>()
    );
    // In the lobby: no rate, hash or players; the pace, and the failed call the tap saw.
    assert_eq!(
        answered(replies),
        [
            ConsoleAnswer::Status(SessionStatus {
                phase: SessionPhase::Lobby,
                rate: None,
                state_hash: None,
                pace: Some(PaceStatus {
                    paused: true,
                    speed: GameSpeed::Double,
                }),
                players: Vec::new(),
                script_failures: ScriptFailureCount {
                    count: 1,
                    latest: Some(FailedScriptCall {
                        tick: 3,
                        unit: None,
                        hook: "on_think".to_owned(),
                        error: "no".to_owned(),
                    }),
                },
            }),
            ConsoleAnswer::Refused(ConsoleRefusal::NoMatch),
            ConsoleAnswer::Refused(ConsoleRefusal::NoMatch),
            ConsoleAnswer::Cancelled,
            ConsoleAnswer::Paced(PaceStatus {
                paused: false,
                speed: GameSpeed::Double,
            }),
        ]
    );
    assert!(!pace.is_paused());

    // A dedicated server's console runs no run and sets no pace.
    let dedicated = lobby(None);
    let pace_ask = ConsoleAsk::Pace(PaceAsk {
        paused: Some(true),
        speed: None,
    });
    assert_eq!(
        answered(answers(&dedicated, [run(RunTo::Tick(5), None), pace_ask])),
        [
            ConsoleAnswer::Refused(ConsoleRefusal::NotLocal),
            ConsoleAnswer::Refused(ConsoleRefusal::NotLocal)
        ]
    );
}

#[test]
fn a_run_plays_until_its_end_and_pauses_there() {
    let pace = Arc::new(Pace::default());
    pace.set_paused(true);
    pace.set_speed(PaceSpeed::Half);
    let mut world = lobby(Some(Arc::clone(&pace)));
    world.init_resource::<EntityIndex>();
    // Tick 2 ran: the next is 3.
    world.insert_resource(SimTick::new(Tick::new(3)));
    let reached = ConsoleAnswer::Ran(RunStop::Reached);
    // A tick that ran, and no tick more, end where they start, the pace as it was.
    let replies = answers(
        &world,
        [
            run(RunTo::Tick(2), None),
            run(RunTo::Ticks(0), None),
            run(
                RunTo::Until(RunCondition::Unit {
                    id: 7,
                    state: UnitActivity::Idle,
                }),
                None,
            ),
        ],
    );
    assert_eq!(replies[0].tick, Some(2));
    assert_eq!(
        answered(replies),
        [
            reached.clone(),
            reached.clone(),
            ConsoleAnswer::Refused(ConsoleRefusal::UnknownUnit { id: 7 })
        ]
    );
    assert_eq!((pace.is_paused(), pace.speed()), (true, PaceSpeed::Half));

    // Three ticks more, 3 to 5: it plays at four times, and another run or a pace waits for it.
    let replies = answers(
        &world,
        [
            run(RunTo::Ticks(3), None),
            run(RunTo::Tick(9), None),
            ConsoleAsk::Pace(PaceAsk {
                paused: Some(true),
                speed: None,
            }),
        ],
    );
    let under_way = ConsoleAnswer::Refused(ConsoleRefusal::RunUnderWay { run: RequestId(1) });
    assert_eq!(answered(replies), [under_way.clone(), under_way]);
    assert_eq!(
        (pace.is_paused(), pace.speed()),
        (false, PaceSpeed::Quadruple)
    );
    let held = |world: &mut World| {
        MatchConsole::hold(world);
        let mut replies = Vec::new();
        world.resource::<MatchConsole>().take_replies(&mut replies);
        replies
    };
    // After tick 4 it runs on; after tick 5 it pauses at the speed before, and answers.
    world.insert_resource(SimTick::new(Tick::new(5)));
    assert_eq!(held(&mut world), []);
    world.insert_resource(SimTick::new(Tick::new(6)));
    assert_eq!(
        held(&mut world),
        [Reply {
            id: RequestId(1),
            tick: Some(5),
            answer: reached
        }]
    );
    assert_eq!((pace.is_paused(), pace.speed()), (true, PaceSpeed::Half));
    assert_eq!(held(&mut world), []);

    // A cancel ends a run at the tick it reached, and so does its wall time, here none.
    answers(&world, [run(RunTo::Tick(100), None)]);
    let replies = answers(&world, [ConsoleAsk::Cancel(RequestId(1))]);
    assert_eq!(answered(replies), [ConsoleAnswer::Cancelled]);
    let ended = held(&mut world);
    assert_eq!(answered(ended), [ConsoleAnswer::Ran(RunStop::Cancelled)]);
    answers(&world, [run(RunTo::Tick(100), Some(0))]);
    assert_eq!(
        answered(held(&mut world)),
        [ConsoleAnswer::Ran(RunStop::WallTime)]
    );

    // A run until a call fails ends after the tick a new failure comes in; the hold of a first
    // tick answers no request.
    answers(
        &world,
        [run(RunTo::Until(RunCondition::ScriptFailure), None)],
    );
    assert_eq!(held(&mut world), []);
    let failed = r#"{"level":"WARN","target":"campfire_runner","fields":{"message":"a script call failed and changed nothing","tick":6,"hook":"on_think","error":"no"}}"#;
    world
        .resource::<MatchConsole>()
        .seen()
        .see(&campfire_log::LogLine::parse(failed).unwrap());
    assert_eq!(
        answered(held(&mut world)),
        [ConsoleAnswer::Ran(RunStop::Held)]
    );
    world.resource::<MatchConsole>().hold_first_tick();
    assert_eq!(held(&mut world), []);
    assert!(world.resource::<MatchConsole>().queues().run.is_none());
    // A speed asked while the first tick's hold waits is the one the hold pauses at; a pause
    // asked ends the hold, so its end undoes neither.
    let pace_ask = |paused, speed| ConsoleAsk::Pace(PaceAsk { paused, speed });
    world.resource::<MatchConsole>().hold_first_tick();
    answers(&world, [pace_ask(None, Some(GameSpeed::Double))]);
    let before = |world: &World| {
        let queues = world.resource::<MatchConsole>().queues();
        queues.run.as_ref().map(|hold| hold.before)
    };
    assert_eq!(before(&world), Some(PaceSpeed::Double));
    answers(&world, [pace_ask(Some(false), None)]);
    assert_eq!(before(&world), None);
}
