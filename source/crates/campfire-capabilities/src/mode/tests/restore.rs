use super::*;
use crate::orders::next_think::NextThink;
use crate::stats::instance::StackEnd;
use crate::stats::lifetime::{Ends, Lifetime};
use crate::stats::modifier_clocks::Interval;

/// Each state type's restore check lets the match's own values through, and refuses one that
/// names what the match lacks or has another shape than the mode's.
#[test]
fn a_restore_check_refuses_what_the_match_lacks() {
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    let grunt = game.entity(2);
    let world = &game.sim.world;
    // Teams a, b and neutral are 0 to 2, players 0 to 2.
    assert!(Team::new(2).check(world, grunt) && !Team::new(3).check(world, grunt));
    let owner = |slot| Owner::new(PlayerSlot::new(slot));
    assert!(owner(2).check(world, grunt) && !owner(3).check(world, grunt));
    let own_type = *world.get::<UnitType>(grunt).unwrap();
    assert!(own_type.check(world, grunt) && !UnitType::new(u16::MAX).check(world, grunt));
    let mut relations = Relations::default();
    relations.set(Team::new(0), Team::new(2), Attitude::Friendly, true);
    assert!(relations.check(world));
    relations.set(Team::new(0), Team::new(3), Attitude::Friendly, true);
    assert!(!relations.check(world));
    let ended = |team| MatchEnd::new(Tick::new(0), MatchResult::Won(Team::new(team)));
    assert!(ended(1).check(world) && !ended(3).check(world));
    // The map has one path, 0.
    let on_path = |path| OnPath::new(PathId::new(path));
    assert!(on_path(0).check(world, grunt) && !on_path(1).check(world, grunt));

    // Choices: a row of 1 + 1 + 2 values for each of 3 players, each an offer of its choice.
    let choices = world.resource::<Choices>().clone();
    assert!(choices.check(world));
    assert!(!Choices(vec![None; 3]).check(world));
    let mut past = choices.clone();
    past.0[0] = Some(Offer::new(99));
    assert!(!past.check(world));
    // The mode's state: its fields' types in the order of their names, `count` an integer first.
    let state = world.resource::<ModeState>().clone();
    assert!(state.check(world));
    assert!(!ModeState(state.0[1..].to_vec()).check(world));
    let mut retyped = state.clone();
    retyped.0[0] = StateValue::Bool(true);
    assert!(!retyped.check(world));
    // The players' resources: a row of each of the mode's resources for each player.
    assert!(world.resource::<PlayerResources>().check(world));
    let rows = |players, resources| PlayerResources::new(players, resources).check(world);
    assert!(!rows(2, RESOURCES.len()) && !rows(3, RESOURCES.len() + 1));

    // Actions: one the book holds, at a rank it has or 0, and an order of a slot it has.
    let book = world.resource::<ActionBook>();
    let strike = book.action_named(0, "strike").unwrap();
    let slots = |action, rank| ActionSlots::new([(action, SlotKind::new(0), rank)]);
    assert!(slots(strike, 0).check(world, grunt) && slots(strike, 1).check(world, grunt));
    assert!(!slots(strike, u8::MAX).check(world, grunt));
    assert!(!slots(ActionId::nth(u32::MAX), 1).check(world, grunt));
    let mut ordered = slots(strike, 1);
    ordered.order(1, ActionTarget::None);
    assert!(!ordered.check(world, grunt));
    let mut queue = TrainQueue::default();
    let queued = Queued {
        action: strike,
        rank: 1,
        time: Ticks::new(1),
    };
    queue.push(queued, Tick::new(0));
    assert!(
        !queue.check(world, grunt),
        "a cast is no train, and a grunt trains none"
    );

    // Tracks: the mode has two, 0 and 1.
    let on_tracks = |track| Experience::new(TrackSet::of([TrackId::new(track).unwrap()]), None);
    assert!(on_tracks(1).check(world, grunt) && !on_tracks(2).check(world, grunt));
    // Track 0 is the `level` track, whose level is the unit's: one that holds its own is
    // refused, one that holds none passes.
    let level_track = |own: bool| {
        let track = TrackId::new(0).unwrap();
        Experience::new(TrackSet::of([track]), (!own).then_some(track))
    };
    assert!(level_track(false).check(world, grunt) && !level_track(true).check(world, grunt));
    let unit = *world.get::<StableId>(grunt).unwrap();
    let level_up = |track| LevelUp {
        unit,
        track: TrackId::new(track).unwrap(),
        level: Level::new(2).unwrap(),
    };
    assert!(LevelUps(vec![level_up(1)]).check(world));
    assert!(!LevelUps(vec![level_up(2)]).check(world));

    // A route of the grunt, of the mode's one kind of walker, until its body grows past it.
    let route = game.sim.world.get::<Route>(grunt).unwrap().clone();
    assert!(route.check(&game.sim.world, grunt));
    let wide = Body::new(Num::int(3)).unwrap();
    game.sim.world.entity_mut(grunt).insert(wide);
    assert!(!route.check(&game.sim.world, grunt));
    assert!(!Destination::default().check(&game.sim.world, grunt));
}

/// Each time a unit, the match's end and a route hold is at most `Tick::LIMIT`, which no match
/// reaches, so no sum of two restored times overflows: each passes at the limit, and fails a tick
/// past it.
#[test]
fn a_restore_check_keeps_each_time_within_the_limit() {
    let game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    let grunt = game.entity(2);
    let world = &game.sim.world;
    let unit = *world.get::<StableId>(grunt).unwrap();
    let past = Tick::new(Tick::LIMIT.get() + 1);
    for (at, holds) in [(Tick::LIMIT, true), (past, false)] {
        assert_eq!(NextThink::new(at).check(world, grunt), holds, "{at}");
        assert_eq!(Respawn { at }.check(world, grunt), holds, "{at}");
        let ended = MatchEnd::new(at, MatchResult::Draw);
        assert_eq!(ended.check(world), holds, "{at}");
        let mut attackers = RecentAttackers::default();
        attackers.record(unit, at, world.resource::<EntityIndex>());
        assert_eq!(attackers.check(world, grunt), holds, "{at}");
        let mut route = world.get::<Route>(grunt).unwrap().clone();
        route.ask(*world.get::<Position>(grunt).unwrap(), at);
        assert_eq!(route.check(world, grunt), holds, "{at}");
    }
}

/// The restore checks of a unit's modifiers, their clocks and the players' modifiers let the
/// match's own through, and refuse one the book lacks or of another shape.
#[test]
fn a_restore_check_refuses_modifiers_the_book_lacks() {
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    // A modifier of the book, with a value for its one change and no state, as the fighter's is;
    // not one the book lacks, nor one of other state or another count of changes.
    let fighter = game.fighter(0, &[("armor", Num::int(1))]);
    let fighter = game.entity(fighter.get());
    let world = &game.sim.world;
    let modifiers = world.get::<Modifiers>(fighter).unwrap();
    assert!(modifiers.check(world, fighter));
    assert!(
        world
            .get::<ModifierClocks>(fighter)
            .unwrap()
            .check(world, fighter)
    );
    let modifier = modifiers.iter().next().unwrap().id;
    let one = StatShare {
        value: Num::int(1),
        live: None,
    };
    let applied = |id, stats: Vec<StatShare>, state: Vec<StateValue>| Application {
        instance: NewInstance {
            stats,
            state,
            ..NewInstance::bare(id, None)
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    };
    let modifiers_of = |application| {
        let mut modifiers = Modifiers::default();
        modifiers.apply(&mut ModifierClocks::default(), application);
        modifiers
    };
    let clocks_of = |application| {
        let mut clocks = ModifierClocks::default();
        Modifiers::default().apply(&mut clocks, application);
        clocks
    };
    let unknown = ModifierId::new(u16::MAX);
    assert!(!modifiers_of(applied(unknown, vec![one], Vec::new())).check(world, fighter));
    assert!(!modifiers_of(applied(modifier, Vec::new(), Vec::new())).check(world, fighter));
    let state = vec![StateValue::Bool(true)];
    assert!(!clocks_of(applied(modifier, vec![one], state)).check(world, fighter));
    // Its end, its stacks' life and end, and its interval's period and next tick are at most
    // the limit, so no sum of two overflows: each passes there, and fails a tick past it.
    let past = Tick::new(Tick::LIMIT.get() + 1);
    let longer = Ticks::new(Ticks::LIMIT.get() + 1);
    let timed = |ends: Tick, life: Ticks, stack_end: Tick, every: Ticks, next: Tick| Application {
        instance: NewInstance {
            stats: vec![one],
            lifetime: Lifetime::new(None, Ends::At(ends)),
            stack_life: Some(life),
            stack_ends: vec![StackEnd {
                until: stack_end,
                count: 1,
            }],
            interval: Some(Interval { every, next }),
            ..NewInstance::bare(modifier, None)
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    };
    // Each case, and whether the modifiers, then the clocks, hold it: an interval is the clocks'.
    let (at, life) = (Tick::LIMIT, Ticks::LIMIT);
    let cases = [
        (timed(at, life, at, life, at), true, true),
        (timed(past, life, at, life, at), false, true),
        (timed(at, longer, at, life, at), false, true),
        (timed(at, life, past, life, at), false, true),
        (timed(at, life, at, longer, at), true, false),
        (timed(at, life, at, life, past), true, false),
    ];
    for (case, (application, modifiers_hold, clocks_hold)) in cases.into_iter().enumerate() {
        let modifiers = modifiers_of(application.clone());
        assert_eq!(
            modifiers.check(world, fighter),
            modifiers_hold,
            "case {case}"
        );
        let clocks = clocks_of(application);
        assert_eq!(clocks.check(world, fighter), clocks_hold, "case {case}");
    }
    let mut held = PlayerModifiers::default();
    held.add(PlayerModifier {
        player: PlayerSlot::new(0),
        modifier: ModifierId::new(0),
    });
    assert!(held.check(world));
    held.add(PlayerModifier {
        player: PlayerSlot::new(0),
        modifier: ModifierId::new(u16::MAX),
    });
    assert!(!held.check(world));
}
