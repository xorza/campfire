use bevy_ecs::component::Component;
use bevy_ecs::world::Mut;
use campfire_actions::{SlotCharges, SlotTimes, TimedSlots};
use campfire_common::Tick;
use campfire_navigation::{Party, PartyKey};
use campfire_orders::NextThink;
use campfire_production::{Gatherer, NodeAt};
use campfire_progression::{LevelUp, LevelUps};
use campfire_sim::{HostRule, Position, SimWorld, SnapshotError, StateDelta, StateRegistry};
use campfire_stats::{Ends, Lifetime, StackEnd};
use campfire_units::{
    DashTo, ForcedMove, MapBook, SpawnPoint, SyncOwner, SyncTeam, UnitEntity, UnitState,
};
use campfire_values::{ActionStart, DashDelivery};

use super::*;

/// Each state type's restore check lets the match's own values through, and refuses one that
/// names what the match lacks or has another shape than the mode's.
#[test]
fn a_restore_check_refuses_what_the_match_lacks() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let grunt = Mode::entity_by_id(&game, 2);
    // The grunt holds strike, as a unit holds the times of the slots it holds.
    let strike = game
        .resource::<ActionBook>()
        .names()
        .named((PackageIndex::MODE, "strike"));
    let strike = ActionSlots::new([(strike.unwrap(), SlotKind::new(0), Rank::new(1))]);
    game.entity_mut(grunt).insert(TimedSlots::new(strike));
    // Teams a, b and neutral are 0 to 2, players 0 to 2: the grunt, which no player owns, may be
    // of team 2, not of team 3.
    let id = *game.get::<StableId>(grunt).unwrap();
    let invalid = |ty| Err(SnapshotError::Invalid { ty, id: Some(id) });
    assert_eq!(game.get::<Owner>(grunt), None);
    assert_eq!(checked_with(&mut game, grunt, Team::new(2)), Ok(()));
    assert_eq!(
        checked_with(&mut game, grunt, Team::new(3)),
        invalid("units.team")
    );
    // Team a holds players 0 and 1, team b player 2. The grunt is of team a: player 0 or 1 owns
    // it, and neither player 2, of team b, nor 3, whom the session lacks.
    assert_eq!(game.get::<Team>(grunt), Some(&Team::new(0)));
    let owner = |slot| Owner::new(PlayerSlot::new(slot));
    assert!(owner(0).check(&game, grunt) && owner(1).check(&game, grunt));
    assert!(!owner(2).check(&game, grunt) && !owner(3).check(&game, grunt));
    let own_type = *game.get::<UnitType>(grunt).unwrap();
    assert_eq!(checked_with(&mut game, grunt, own_type), Ok(()));
    // A type the match did not load: the grunt's state classes, which the type lacks, refuse it
    // first. Without them, the type's own check refuses it, and the grunt's own type, whose
    // classes it lacks.
    let unloaded = UnitType::new(u16::MAX);
    let class = invalid("units.state_all");
    assert_eq!(checked_with(&mut game, grunt, unloaded), class);
    let mut unit = game.entity_mut(grunt);
    let classes = unit
        .take::<(UnitState<SyncOwner>, UnitState<SyncAll>)>()
        .unwrap();
    let refused = invalid("units.unit_type");
    assert_eq!(checked_with(&mut game, grunt, unloaded), refused);
    assert_eq!(checked_with(&mut game, grunt, own_type), refused);
    game.entity_mut(grunt).insert(classes);
    assert_eq!(game.resource::<StateRegistry>().check(&game), Ok(()));
    // The map has one path, 0.
    let on_path = |path| OnPath::new(PathId::new(path));
    assert!(on_path(0).check(&game, grunt) && !on_path(1).check(&game, grunt));
    // A unit respawns where it spawned: on the map's far corner, or a meter past it along x.
    let [x, z] = game.resource::<MapBook>().bounds.max();
    let point = |x: Num| SpawnPoint::new(Vec3::new(x, Num::ZERO, z));
    assert!(point(x).check(&game, grunt));
    assert!(!point(x + Num::ONE).check(&game, grunt));

    // Choices: a row of 1 + 1 + 2 values for each of 3 players, each an offer of its choice.
    let mut choices = game.resource::<Choices>().clone();
    assert!(choices.check(&game));
    assert!(!Choices::of(vec![None; 3]).check(&game));
    choices.offers_mut()[0] = Some(Offer::new(99));
    assert!(!choices.check(&game));
    // Each row is `duo`'s two values, then `hero`'s, then `spells`'s, by their names. A choice is
    // all chosen or none: half of `duo` is refused. Its two values differ. A unique `hero` is one
    // player's: the same hero in two rows is refused, while a `duo` two players share restores.
    let chose = |values: &[(usize, usize)]| {
        let mut choices = Choices::of(vec![None; 12]);
        for &(at, offer) in values {
            choices.offers_mut()[at] = Some(Offer::new(offer));
        }
        choices.check(&game)
    };
    assert!(chose(&[(0, 0), (1, 1), (2, 0)]));
    assert!(!chose(&[(0, 0)]));
    assert!(!chose(&[(0, 1), (1, 1)]));
    assert!(chose(&[(0, 0), (1, 1), (4, 0), (5, 1)]));
    assert!(chose(&[(2, 0), (6, 1)]));
    assert!(!chose(&[(2, 0), (6, 0)]));
    // A grunt's state: `hits`, an integer, its owner's, and `mark`, a text, of all; neither class
    // a field short, nor a value of another type, and no class it declares no field of.
    let owner_state = game.get::<UnitState<SyncOwner>>(grunt).unwrap();
    let all_state = game.get::<UnitState<SyncAll>>(grunt).unwrap();
    assert!(owner_state.check(&game, grunt) && all_state.check(&game, grunt));
    let int = || vec![StateValue::Int(0)];
    let text = || vec![StateValue::Text("".into())];
    assert!(!UnitState::<SyncOwner>::new(vec![]).check(&game, grunt));
    assert!(!UnitState::<SyncOwner>::new(text()).check(&game, grunt));
    assert!(!UnitState::<SyncOwner>::new([int(), text()].concat()).check(&game, grunt));
    assert!(!UnitState::<SyncAll>::new(int()).check(&game, grunt));
    assert!(!UnitState::<SyncTeam>::new(int()).check(&game, grunt));
    // Actions: one the book holds, at a rank it has or 0, and an order of a slot it has.
    let book = game.resource::<ActionBook>();
    let strike = book.names().named((PackageIndex::MODE, "strike")).unwrap();
    let slots = |action, rank| ActionSlots::new([(action, SlotKind::new(0), Rank::new(rank))]);
    assert!(slots(strike, 0).check(&game, grunt) && slots(strike, 1).check(&game, grunt));
    assert!(!slots(strike, u8::MAX).check(&game, grunt));
    assert!(!slots(ActionId::new(u32::MAX), 1).check(&game, grunt));
    let mut ordered = slots(strike, 1);
    ordered.order(1, ActionTarget::None);
    assert!(!ordered.check(&game, grunt));
    // Times of each slot the grunt has, one; a charge only on an action with them, which strike
    // is not.
    let times = game.get::<SlotTimes>(grunt).unwrap();
    assert!(times.check(&game, grunt));
    let mut charged = times.clone();
    let one = SlotCharges {
        count: 1,
        next: Tick::ZERO,
    };
    charged.set_charges(0, Some(one));
    assert!(!charged.check(&game, grunt));
    let none = SlotTimes::ready(&ActionSlots::new([]));
    assert!(!none.check(&game, grunt));
    let mut queue = TrainQueue::default();
    let queued = Queued {
        action: strike,
        rank: Rank::FIRST,
        time: Ticks::new(1),
        paid: 0,
    };
    queue.push(queued, &[], Tick::new(0));
    assert!(
        !queue.check(&game, grunt),
        "a cast is no train, and a grunt trains none"
    );

    // Tracks: the mode has two, 0 and 1.
    let on_tracks = |track| Experience::new(TrackSet::from([TrackId::new(track)]));
    assert!(on_tracks(1).check(&game, grunt) && !on_tracks(2).check(&game, grunt));
    // Track 0 is the `level` track, whose level is the unit's, 1 for the grunt: the level 1 there
    // passes, a level 2 is refused.
    assert_eq!(game.get::<Level>(grunt).unwrap().get(), 1);
    let mut ahead = on_tracks(0);
    assert!(ahead.check(&game, grunt));
    ahead.tracks_mut()[0].level = Level::new(2).unwrap();
    assert!(!ahead.check(&game, grunt));
    let id = *game.get::<StableId>(grunt).unwrap();
    let level_up = |track| LevelUp {
        id,
        track: TrackId::new(track),
        level: Level::new(2).unwrap(),
    };
    assert!(LevelUps([level_up(1)].into()).check(&game));
    assert!(!LevelUps([level_up(2)].into()).check(&game));

    // A route of the grunt, of the mode's one kind of walker, until its body grows past it.
    let route = game.get::<Route>(grunt).unwrap().clone();
    assert!(route.check(&game, grunt));
    let wide = Body::new(Num::int(3)).unwrap();
    game.entity_mut(grunt).insert(wide);
    assert!(!route.check(&game, grunt));
}

/// The match's and the players' entities stand at their named ids, and each state type on them
/// keeps to its entity and to the mode's shape.
#[test]
fn a_restore_check_keeps_the_match_and_its_players_to_their_entities() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let grunt = Mode::entity_by_id(&game, 2);
    // The match's entity and each player's stand at their named ids, of a player the session
    // has; a unit stands at an allocated id.
    let match_entity = game.match_entity().unwrap();
    let first = game.player_entity(PlayerSlot::new(0)).unwrap();
    let player = |slot| Player::new(PlayerSlot::new(slot));
    assert!(MatchEntity.check(&game, match_entity) && !MatchEntity.check(&game, first));
    assert!(player(0).check(&game, first) && !player(1).check(&game, first));
    assert!(!player(0).check(&game, grunt));
    assert!(UnitEntity.check(&game, grunt) && !UnitEntity.check(&game, first));
    // Each state type sits on its kind of entity: the match's parts on its entity, a player's on
    // a player's, a unit's on a unit, and a team on a unit or a player's entity.
    let on = |hosted: fn(&World, Entity) -> bool| {
        [match_entity, first, grunt].map(|entity| hosted(&game, entity))
    };
    assert_eq!(on(hosts::<MatchEnd>), [true, false, false]);
    assert_eq!(on(hosts::<ModeState<SyncNone>>), [true, false, false]);
    assert_eq!(on(hosts::<ModeState<SyncAll>>), [true, false, false]);
    assert_eq!(on(hosts::<PlayerResources>), [false, true, false]);
    assert_eq!(on(hosts::<Life>), [false, false, true]);
    // A team sits on a unit or a player's entity, and the relations on the match's, as `units`
    // declares their hosts.
    let wrong_host = |ty, entity| {
        let id = *game.get::<StableId>(entity).unwrap();
        Err(SnapshotError::WrongHost { ty, id })
    };
    let team_on_match = wrong_host("units.team", match_entity);
    let relations_on_player = wrong_host("units.relations", first);
    assert_eq!(
        checked_with(&mut game, match_entity, Team::new(0)),
        team_on_match
    );
    let relations = Relations::default();
    assert_eq!(
        checked_with(&mut game, first, relations),
        relations_on_player
    );
    // The relations and the match's end, on the match's entity, of the mode's teams.
    let match_id = *game.get::<StableId>(match_entity).unwrap();
    let mut relations = Relations::default();
    relations.set(Team::new(0), Team::new(2), Relation::Friendly, true);
    assert_eq!(
        checked_with(&mut game, match_entity, relations.clone()),
        Ok(())
    );
    relations.set(Team::new(0), Team::new(3), Relation::Friendly, true);
    let unknown = Err(SnapshotError::Invalid {
        ty: "units.relations",
        id: Some(match_id),
    });
    assert_eq!(checked_with(&mut game, match_entity, relations), unknown);
    let ended = |team| MatchEnd::new(Tick::new(0), MatchResult::Won(Team::new(team)));
    assert!(ended(1).check(&game, match_entity) && !ended(3).check(&game, match_entity));
    // The mode's state, on the match's entity, each class its fields of their types in the order
    // of their names: `kind`, a text, and `seen`, an integer, kept on the server; the rest sent
    // to all, `count`, an integer, first.
    let none = game.get::<ModeState<SyncNone>>(match_entity).unwrap();
    let all = game.get::<ModeState<SyncAll>>(match_entity).unwrap();
    assert!(none.check(&game, match_entity) && all.check(&game, match_entity));
    let text = StateValue::Text("".into());
    assert_eq!(none.values(), [text, StateValue::Int(0)]);
    let class = |values: &[StateValue]| {
        ModeState::<SyncAll>::new(values.to_vec()).check(&game, match_entity)
    };
    assert!(!class(&all.values()[1..]) && !class(none.values()));
    let mut retyped = all.values().to_vec();
    retyped[0] = StateValue::Bool(true);
    assert!(!class(&retyped));
    // A player's resources: one of each of the mode's resources, on the player's entity.
    let held = game.get::<PlayerResources>(first).unwrap();
    assert!(held.check(&game, first));
    assert!(!PlayerResources::new(RESOURCES.len() + 1).check(&game, first));
}

/// A snapshot of a match that lacks a part its start spawned fails its restore, by the type whose
/// check finds it missing, where a tick would find it gone.
#[test]
fn a_restore_refuses_a_match_without_a_part_its_start_spawned() {
    assert_eq!(
        refused_snapshot(&mode_match(SCRIPT, ScriptLimits::ROOMY)),
        None
    );
    // Each case breaks the match and names the error that refuses it.
    let cases: [fn(&mut World) -> SnapshotError; 13] = [
        |world| {
            let entity = world.match_entity().unwrap();
            world.entity_mut(entity).remove::<Relations>();
            invalid(Relations::NAME, Some(StableId::MATCH))
        },
        |world| {
            let entity = world.match_entity().unwrap();
            world.entity_mut(entity).remove::<ModeState<SyncAll>>();
            invalid(ModeState::<SyncAll>::NAME, Some(StableId::MATCH))
        },
        // A seat the session has with no player's entity is a breach of the whole match.
        |world| {
            let entity = world.player_entity(PlayerSlot::new(2)).unwrap();
            world.despawn(entity);
            invalid(Player::NAME, None)
        },
        |world| {
            let entity = world.player_entity(PlayerSlot::new(0)).unwrap();
            world.entity_mut(entity).remove::<PlayerResources>();
            let id = StableId::of_player(PlayerSlot::new(0));
            invalid(PlayerResources::NAME, Some(id))
        },
        |world| {
            let entity = world.player_entity(PlayerSlot::new(0)).unwrap();
            world.entity_mut(entity).remove::<PlayerModifiers>();
            let id = StableId::of_player(PlayerSlot::new(0));
            invalid(PlayerModifiers::NAME, Some(id))
        },
        // A unit: one the mode spawned loses its level, which stats declares for it, a unit
        // loses a class of the state its type declares, or its team; a living unit holds a
        // respawn; a unit's life sits on the match's entity; slots come without their times.
        |world| {
            let unit = first::<Level>(world);
            world.entity_mut(unit).remove::<Level>();
            invalid(Level::NAME, Some(id_of(world, unit)))
        },
        |world| {
            let unit = first::<UnitState<SyncOwner>>(world);
            world.entity_mut(unit).remove::<UnitState<SyncOwner>>();
            invalid(UnitType::NAME, Some(id_of(world, unit)))
        },
        |world| {
            let unit = first::<UnitType>(world);
            world.entity_mut(unit).remove::<Team>();
            lacks(UnitEntity::NAME, Team::NAME, id_of(world, unit))
        },
        |world| {
            let unit = first::<UnitType>(world);
            world.entity_mut(unit).insert(Respawn { at: Tick::new(5) });
            lacks(Respawn::NAME, Dead::NAME, id_of(world, unit))
        },
        |world| {
            let unit = first::<Life>(world);
            let life = world.entity_mut(unit).take::<Life>().unwrap();
            let entity = world.match_entity().unwrap();
            world.entity_mut(entity).insert(life);
            SnapshotError::WrongHost {
                ty: Life::NAME,
                id: StableId::MATCH,
            }
        },
        |world| {
            let strike = world
                .resource::<ActionBook>()
                .names()
                .named((PackageIndex::MODE, "strike"))
                .unwrap();
            let slots = ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))]);
            let unit = first::<UnitType>(world);
            world.entity_mut(unit).insert(slots);
            lacks(ActionSlots::NAME, SlotTimes::NAME, id_of(world, unit))
        },
        // A unit a bit past the world's bound of 2²⁰ m above the ground: within the map's
        // bounds on the ground plane, and refused, as no place lies past the world's bound.
        |world| {
            let unit = first::<UnitType>(world);
            let height = Position::BOUND + Num::EPSILON;
            *world.get_mut::<Position>(unit).unwrap() =
                Position::from(Vec3::new(Num::ZERO, height, Num::ZERO));
            invalid(Position::NAME, Some(id_of(world, unit)))
        },
        // A player's entity takes a unit's kind and place.
        |world| {
            let entity = world.player_entity(PlayerSlot::new(0)).unwrap();
            let pos = Vec3::ZERO;
            world
                .entity_mut(entity)
                .insert((UnitEntity, Position::from(pos)));
            let id = StableId::of_player(PlayerSlot::new(0));
            invalid(UnitEntity::NAME, Some(id))
        },
    ];
    for strip in cases {
        let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
        let expected = strip(&mut game);
        assert_eq!(refused_snapshot(&game), Some(expected));
    }
}

/// Each time a unit, the match's end and a route hold is at most `Tick::LIMIT`, which no match
/// reaches, so no sum of two restored times overflows: each decodes at the limit, and fails to a
/// tick past it.
#[test]
fn a_restore_keeps_each_time_within_the_limit() {
    let game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let grunt = Mode::entity_by_id(&game, 2);
    let unit = *game.get::<StableId>(grunt).unwrap();
    let past = Tick::new(Tick::LIMIT.get() + 1);
    for (at, holds) in [(Tick::LIMIT, true), (past, false)] {
        assert_eq!(Match::decodes(&NextThink::new(at)), holds, "{at}");
        assert_eq!(Match::decodes(&Respawn { at }), holds, "{at}");
        assert_eq!(
            Match::decodes(&MatchEnd::new(at, MatchResult::Draw)),
            holds,
            "{at}"
        );
        let mut attackers = RecentAttackers::default();
        attackers.record(unit, at, game.resource::<EntityIndex>());
        assert_eq!(Match::decodes(&attackers), holds, "{at}");
        let mut route = game.get::<Route>(grunt).unwrap().clone();
        route.ask(game.get::<Position>(grunt).unwrap().get(), at, None);
        assert_eq!(Match::decodes(&route), holds, "{at}");
    }
}

/// The restore checks of a unit's modifiers, their clocks and the players' modifiers let the
/// match's own through, and refuse one the book lacks or of another shape.
#[test]
fn a_restore_check_refuses_modifiers_the_book_lacks() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    // A modifier of the book, with a value for its one change and no state, as the fighter's is;
    // not one the book lacks, nor one of other state or another count of changes.
    let fighter = Mode::stat_grunt(&mut game, Team::new(0), &[("armor", Num::int(1))]);
    let fighter = Mode::entity_by_id(&game, fighter.get());
    let modifiers = game.get::<Modifiers>(fighter).unwrap();
    assert!(modifiers.check(&game, fighter));
    assert!(
        game.get::<ModifierClocks>(fighter)
            .unwrap()
            .check(&game, fighter)
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
    assert!(!modifiers_of(applied(unknown, vec![one], Vec::new())).check(&game, fighter));
    // Rally reads its action's `ward`, which an instance of it from no action lacks; its next
    // application would fail.
    let rally = Stats::modifier(&game, PackageIndex::MODE, "rally").unwrap();
    assert!(!modifiers_of(applied(rally, Vec::new(), Vec::new())).check(&game, fighter));
    assert!(!modifiers_of(applied(modifier, Vec::new(), Vec::new())).check(&game, fighter));
    let state = vec![StateValue::Bool(true)];
    assert!(!clocks_of(applied(modifier, vec![one], state)).check(&game, fighter));
    // Its end, and its stacks' life and end, are at most the limit, so no sum of two overflows:
    // each restores there, and fails to decode a tick past it.
    let past = Tick::new(Tick::LIMIT.get() + 1);
    let longer = Ticks::new(Ticks::LIMIT.get() + 1);
    let timed = |ends: Tick, life: Ticks, stack_end: Tick| Application {
        instance: NewInstance {
            stats: vec![one],
            lifetime: Lifetime::applied(Ends::At(ends)),
            stack_life: Some(life),
            stack_end: Some(StackEnd {
                until: stack_end,
                count: 1,
            }),
            ..NewInstance::bare(modifier, None)
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    };
    let (at, life) = (Tick::LIMIT, Ticks::LIMIT);
    let cases = [
        (timed(at, life, at), true),
        (timed(past, life, at), false),
        (timed(at, longer, at), false),
        (timed(at, life, past), false),
    ];
    for (case, (application, holds)) in cases.into_iter().enumerate() {
        let modifiers = modifiers_of(application);
        assert_eq!(
            Match::decodes(&modifiers) && modifiers.check(&game, fighter),
            holds,
            "case {case}"
        );
    }
    // A player's modifiers: on its entity, each of the book, and none that reads an action's
    // param.
    let player = game.player_entity(PlayerSlot::new(0)).unwrap();
    let mut held = PlayerModifiers::default();
    held.add(ModifierId::new(0));
    let hosted = <PlayerModifiers as SimComponent>::Host::holds;
    assert!(
        held.check(&game, player)
            && hosted(&game, player)
            && !hosted(&game, Mode::entity_by_id(&game, 0))
    );
    held.add(ModifierId::new(u16::MAX));
    assert!(!held.check(&game, player));
    let mut rallied = PlayerModifiers::default();
    rallied.add(rally);
    assert!(!rallied.check(&game, player));
}

#[test]
fn a_quiet_tick_changes_no_state_but_the_tick() {
    // A mode with a death hook and a timer each tick, on its map's tower alone: once its start
    // settles, a tick with no input and no event writes no state but the tick, so its copy
    // carries nothing more; the timer's call reads the mode's state and writes none, so only the
    // timer, set again, changes besides.
    let script = r#"
fn on_match_start(ctx) {
    ctx.timer("each", 100, true, ());
}

fn on_timer(ctx, name, data) {
    if ctx.state.phase == "never" {
        ctx.state.phase = "now";
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[]);
    Mode::tick_inputs(&mut game, &[]);
    let mut delta = StateDelta::default();
    game.resource_scope(|world, mut registry: Mut<'_, StateRegistry>| {
        registry.track(world, &mut delta);
    });
    Mode::tick_inputs(&mut game, &[]);
    let changed = game.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
        registry.changes(world, &mut delta);
        registry.changed_types(world, &delta)
    });
    assert_eq!(changed, ["mode.timers", "sim.tick"]);
}

/// What a restore's checks say of `game` with `value` on `entity`, which then holds again what it
/// held.
fn checked_with<C: Component + Clone>(
    game: &mut Match,
    entity: Entity,
    value: C,
) -> Result<(), SnapshotError> {
    let held = game.get::<C>(entity).cloned();
    game.entity_mut(entity).insert(value);
    let checked = game.resource::<StateRegistry>().check(game);
    let mut holder = game.entity_mut(entity);
    match held {
        Some(held) => holder.insert(held),
        None => holder.remove::<C>(),
    };
    checked
}

/// Whether `entity` is of the kind `C` sits on.
fn hosts<C: SimComponent>(world: &World, entity: Entity) -> bool {
    C::Host::holds(world, entity)
}

/// The first entity that holds a `C`.
/// Each state type that holds a place refuses one a bit past the world's bound of 2²⁰ m, as no
/// place lies past it, and takes one at the bound: a destination, a dash's point and the start
/// of the action it delivers, a route's goal and its party's, an ordered point, and a gatherer's
/// last node.
#[test]
fn a_place_past_the_world_bound_fails_its_types_check() {
    let game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let grunt = Mode::entity_by_id(&game, 2);
    let id = *game.get::<StableId>(grunt).unwrap();
    let past = Vec3::new(Num::ZERO, Num::ZERO, Position::BOUND + Num::EPSILON);
    let at = Vec3::new(Num::ZERO, Num::ZERO, Position::BOUND);
    let start = |origin| ActionStart {
        origin,
        charge: None,
    };
    let party = |goal| Party {
        key: PartyKey::Spawn(id),
        goal,
    };
    let strike = game
        .resource::<ActionBook>()
        .names()
        .named((PackageIndex::MODE, "strike"))
        .unwrap();
    for (point, holds) in [(at, true), (past, false)] {
        let destination = Destination::to(Some(point));
        assert_eq!(destination.check(&game, grunt), holds, "{point:?}");
        let dash = |to, start| ForcedMove::Dash {
            to,
            step: Num::ONE,
            delivers: Some(DashDelivery::new(id, ActionId::new(0), Rank::FIRST, start)),
        };
        assert_eq!(dash(DashTo::Point(point), None).check(&game, grunt), holds);
        let started = dash(DashTo::Unit(id), Some(start(point)));
        assert_eq!(started.check(&game, grunt), holds);
        let mut route = game.get::<Route>(grunt).unwrap().clone();
        route.ask(point, Tick::new(0), None);
        assert_eq!(route.check(&game, grunt), holds, "{point:?}");
        route.ask(at, Tick::new(0), Some(party(point)));
        assert_eq!(route.check(&game, grunt), holds, "{point:?}");
        let mut slots = ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))]);
        slots.order(0, ActionTarget::Point(point));
        assert_eq!(slots.check(&game, grunt), holds, "{point:?}");
        let last = NodeAt {
            node: id,
            pos: point,
        };
        let gatherer = Gatherer::of_parts(None, None, Some(last));
        assert_eq!(gatherer.check(&game, grunt), holds, "{point:?}");
    }
}

fn first<C: Component>(world: &mut World) -> Entity {
    let mut holders = world.query_filtered::<Entity, With<C>>();
    holders.iter(world).next().unwrap()
}

/// How a fresh match of the script refuses the snapshot of `game`; none when it restores.
fn refused_snapshot(game: &Match) -> Option<SnapshotError> {
    let mut snapshot = Vec::new();
    game.snapshot(&mut snapshot);
    let mut fresh = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let index = fresh.resource::<EntityIndex>();
    let held: Vec<Entity> = index.iter().map(|(_, entity)| entity).collect();
    for entity in held {
        fresh.despawn(entity);
    }
    fresh.restore(&snapshot).err()
}

/// The stable id of `entity`.
fn id_of(world: &World, entity: Entity) -> StableId {
    *world.get::<StableId>(entity).unwrap()
}

/// The error of a snapshot whose entity of `id` holds `holder` and lacks `needs`.
const fn lacks(holder: &'static str, needs: &'static str, id: StableId) -> SnapshotError {
    SnapshotError::Lacks { holder, needs, id }
}

/// The error of a snapshot whose value of `ty` breaks its rules, on the entity of `id`.
const fn invalid(ty: &'static str, id: Option<StableId>) -> SnapshotError {
    SnapshotError::Invalid { ty, id }
}
