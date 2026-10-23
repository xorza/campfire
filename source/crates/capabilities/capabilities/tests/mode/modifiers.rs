use campfire_common::Tick;
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::StatsRefusal;
use campfire_values::Named;

use super::*;

#[test]
fn a_mode_applies_a_modifier_writes_its_handle_and_sees_it_end() {
    let blesser = r#"
fn take_input(ctx, player, name, value) {
    let hero = ctx.avatars()[0];
    if value == "bless" {
        let m = ctx.add_modifier(hero, "blessing", 100);
        ctx.state.seen = m.stacks;
        m.stacks = 3;
        m.state.count = m.state.count + 5;
    } else if value == "again" {
        ctx.state.seen = ctx.add_modifier(hero, "blessing").stacks;
    } else if value == "check" {
        ctx.state.seen = if hero.has_modifier("blessing") { 1 } else { 0 };
    } else if value == "twice" {
        let first = ctx.add_modifier(hero, "blessing");
        let second = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = first.stacks * 10 + second.stacks;
    } else if value == "renew" {
        let m = ctx.add_modifier(hero, "blessing");
        m.state.count = 9;
        ctx.remove_modifier(m);
        let renewed = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = renewed.stacks * 10 + renewed.state.count;
    } else if value == "find" {
        let m = hero.modifier("blessing");
        ctx.state.seen = if m == () { -1 } else { m.stacks * 10 + m.state.count };
    } else if value == "find_from" {
        ctx.state.seen = if hero.modifier("blessing", hero) == () { -1 } else { 1 };
    } else if value == "find_twice" {
        let m = hero.modifier("blessing");
        m.stacks = 2;
        ctx.state.seen = hero.modifier("blessing").stacks;
    } else if value == "drop" {
        ctx.remove_modifier(hero.modifier("blessing"));
    } else if value == "zero" {
        let m = hero.modifier("blessing");
        m.stacks = 0;
    } else if value == "unknown" {
        ctx.add_modifier(hero, "curse");
    } else if value == "rally" {
        ctx.add_modifier(hero, "ral" + "ly");
    }
}
"#;
    let mut game = picking_match(blesser, ScriptLimits::ROOMY);
    let hero = Mode::pick(&mut game, 0, "hero-x");
    let held = |game: &Match| {
        let modifiers = game.get::<Modifiers>(hero).unwrap();
        let clocks = game.get::<ModifierClocks>(hero).unwrap();
        modifiers
            .iter()
            .enumerate()
            .map(|(at, instance)| {
                (
                    instance.stacks,
                    instance.lifetime.until().map(Tick::get),
                    clocks.state(at).to_vec(),
                )
            })
            .collect::<Vec<_>>()
    };
    // In tick t a new one, 1 stack as the call sees it, written to 3 and its count from 0 to 5;
    // 100 ms at 10 ticks a second is 1 tick, so it holds through t + 1 and ends as t + 2
    // starts.
    let t = game.now().get();
    Mode::tick_inputs(&mut game, &[(0, input("probe", "bless"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(1));
    assert_eq!(held(&game), [(3, Some(t + 2), vec![StateValue::Int(5)])]);
    // Again in t + 1 with its own 200 ms, 2 ticks: a fourth stack, the limit, and the end
    // t + 1 + 2 + 1; the count stays.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "again"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(4));
    assert_eq!(held(&game), [(4, Some(t + 4), vec![StateValue::Int(5)])]);
    Mode::tick_inputs(&mut game, &[(0, input("probe", "check"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(1));
    // Through t + 3, then gone as t + 4 starts: once that tick has run.
    while game.now().get() <= t + 4 {
        Mode::tick_inputs(&mut game, &[]);
    }
    assert_eq!(held(&game), []);
    Mode::tick_inputs(&mut game, &[(0, input("probe", "check"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(0));
    // Two applications in one call in tick u: one handle, which sees both, 2 stacks; 200 ms, so
    // it ends as u + 3 starts.
    let u = game.now().get();
    Mode::tick_inputs(&mut game, &[(0, input("probe", "twice"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(22));
    assert_eq!(held(&game), [(2, Some(u + 3), vec![StateValue::Int(0)])]);
    // In u + 1, a third stack written, removed, and applied again: a new one, 1 stack and its
    // count 0, which ends as u + 4 starts.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "renew"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(10));
    assert_eq!(held(&game), [(1, Some(u + 4), vec![StateValue::Int(0)])]);
    // The hero's instance, which the mode applied from no source, is the one `modifier` finds
    // and none from the hero: its two finds in a call are one handle, whose 2 stacks it writes,
    // and the next call reads 2 stacks and the count 0. A removal through it ends it.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "find_twice"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(2));
    for (probe, seen) in [("find", 20), ("find_from", -1)] {
        Mode::tick_inputs(&mut game, &[(0, input("probe", probe))]);
        assert_eq!(Mode::field(&game, "seen"), StateValue::Int(seen), "{probe}");
    }
    Mode::tick_inputs(&mut game, &[(0, input("probe", "drop"))]);
    assert_eq!(held(&game), []);
    Mode::tick_inputs(&mut game, &[(0, input("probe", "find"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(-1));
    // Written no stack, a new one ends with its clock, as its last stack's end would: the hero
    // holds it no more.
    for probe in ["bless", "zero"] {
        Mode::tick_inputs(&mut game, &[(0, input("probe", probe))]);
    }
    assert_eq!(held(&game), []);
    Mode::tick_inputs(&mut game, &[(0, input("probe", "check"))]);
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(0));
    // A name the package does not declare fails; so does one the script computes, of a modifier
    // that reads a param of its applier's action, which the mode does not have.
    Mode::tick_inputs(
        &mut game,
        &[(0, input("probe", "unknown")), (0, input("probe", "rally"))],
    );
    let missing = StatsRefusal::param(ParamProblem::Missing).refused();
    let refused = [ApiError::Unknown(ModifierId::NAMES), missing];
    assert_eq!(Mode::failures(&game), refused.map(FailureKind::Api));
}

#[test]
fn a_player_modifier_holds_on_each_unit_of_its_player_it_selects_and_on_one_spawned_after() {
    let script = r#"
fn on_input(ctx, player, name, value) {
    let at = ctx.map.markers("camp")[0].position;
    let players = ctx.players;
    if value == "units" {
        ctx.spawn_unit("grunt", "a", at, players[1]);
        ctx.spawn_unit("tower", "a", at, players[1]);
        ctx.spawn_unit("grunt", "a", at, players[0]);
    } else if value == "drill" {
        ctx.add_player_modifier(players[1], "drill");
    } else if value == "another" {
        ctx.spawn_unit("grunt", "a", at, players[1]);
    } else if value == "undrill" {
        ctx.remove_player_modifier(players[1], "drill");
        ctx.remove_player_modifier(players[0], "drill");
    } else if value == "stranger" {
        ctx.add_player_modifier(players[1], "march");
    } else if value == "rally" {
        ctx.add_player_modifier(players[1], "ral" + "ly");
    } else if value == "turncoat" {
        ctx.spawn_unit("grunt", "a", at, players[2]);
    }
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    let drill = Stats::modifier(&game, PackageIndex::MODE, "drill").unwrap();
    // Units 1 to 3, after the map's tower: a grunt and a tower of player 1, and a grunt of
    // player 0.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "units"))]);
    let drilled = |game: &Match| {
        let mut held = Vec::new();
        for (id, entity) in game.resource::<EntityIndex>().iter() {
            let Some(modifiers) = game.get::<Modifiers>(entity) else {
                continue;
            };
            if let Some(instance) = modifiers.get(drill, None) {
                assert!(
                    instance.lifetime.held_by(Hold::Player) && instance.lifetime.until().is_none()
                );
                held.push(id.get());
            }
        }
        held
    };
    assert!(drilled(&game).is_empty());
    // From the tick player 1 holds the drill, its grunt holds it, from no source, with no end;
    // its tower is no grunt, and player 0's grunt is not its.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "drill"))]);
    assert_eq!(drilled(&game), [1]);
    // A grunt of player 1 spawned later holds it from its first Resolve.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "another"))]);
    assert_eq!(drilled(&game), [1, 4]);
    // Grunt 1 goes to player 0, and holds the drill no more. Grunt 4 dies, and its modifiers
    // but its passives end, the drill among them; it holds it again once it lives.
    let unit = |game: &Match, number| {
        let index = game.resource::<EntityIndex>();
        index.iter().find(|(id, _)| id.get() == number).unwrap()
    };
    let (first, _) = unit(&game, 1);
    let (fourth, revived) = unit(&game, 4);
    game.insert_parts(first, Owner::new(PlayerSlot::new(0)));
    game.insert_parts(fourth, Dead);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(drilled(&game), Vec::<u64>::new());
    game.entity_mut(revived).remove::<Dead>();
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(drilled(&game), [4]);
    let held = |game: &Match, slot| {
        let entity = game.player_entity(PlayerSlot::new(slot)).unwrap();
        game.get::<PlayerModifiers>(entity)
            .unwrap()
            .iter()
            .collect::<Vec<_>>()
    };
    assert_eq!((held(&game, 1), held(&game, 0)), (vec![drill], vec![]));
    // Player 1's removal ends the drill on grunt 4 in the tick's Resolve, and player 0's, which
    // holds none, does nothing.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "undrill"))]);
    assert!(Mode::failures(&game).is_empty());
    assert_eq!((held(&game, 1), held(&game, 0)), (vec![], vec![]));
    assert_eq!(drilled(&game), Vec::<u64>::new());
    // A modifier the package does not declare, a modifier that reads a param of an action,
    // which a player's never has, and a unit of player 2, of team `b`, on team `a`, fail, and
    // the last spawns nothing.
    Mode::tick_inputs(
        &mut game,
        &[
            (1, input("probe", "stranger")),
            (1, input("probe", "rally")),
            (1, input("probe", "turncoat")),
        ],
    );
    let missing = StatsRefusal::param(ParamProblem::Missing).refused();
    let refused = [
        ApiError::Unknown(ModifierId::NAMES),
        missing,
        ModeRefusal::PlayerOffTeam.refused(),
    ];
    assert_eq!(Mode::failures(&game), refused.map(FailureKind::Api));
    // The five units, then the match's entity and the three players'.
    assert_eq!(game.resource::<EntityIndex>().iter().count(), 5 + 4);
}
