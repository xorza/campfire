use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::Mut;
use campfire_common::Tick;
use campfire_sim::{SimEdge, SimSet, SimUpdate};

use super::*;

#[test]
fn a_death_reaches_the_mode_and_a_respawn_brings_the_unit_back_at_its_spawn() {
    // The mode spawns as the test mode does, records each death, and respawns the dead 250 ms
    // later: 3 ticks at 10 a second. A probe respawns the first unit with the tag it names.
    let script = r#"
fn on_match_start(ctx) {
    for camp in ctx.map.markers("camp") {
        ctx.spawn_unit(camp.params.unit_type, "neutral", camp.pos);
    }
    ctx.spawn_group("a", "mid", PathEnd::Start, ctx.p.group);
    ctx.spawn_group("b", "mid", PathEnd::End, ["grunt"]);
}

fn on_unit_died(ctx, unit, killer, assisters) {
    ctx.state.count += 1;
    ctx.state.kind = unit.unit_type;
    ctx.state.team = killer.team;
    ctx.state.grunts = assisters.len();
    ctx.state.path = assisters[0].team;
    ctx.respawn(unit, 250);
}

fn on_mode_input(ctx, player, name, value) {
    ctx.respawn(ctx.units_tagged(value)[0], 100);
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    // Units: the tower 0 of a, the neutral grunt 1 at (0, 0), a's grunts 2 and 3, b's grunt 4. The
    // neutral grunt stands at (3, 0) when b's grunt strikes it for its 10 health, in tick 0; a's
    // grunt 2 struck it in the same tick, within the window of 10 ticks.
    let victim = Mode::entity_by_id(&game, 1);
    *game.get_mut::<Position>(victim).unwrap() = Position::from((3, 0, 0));
    game.resource_mut::<CombatBook>().assist_window = Some(Ticks::new(10));
    let ids: Vec<_> = game
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, _)| id)
        .collect();
    let (one, two, four) = (ids[1], ids[2], ids[4]);
    game.resource_scope(|world, index: Mut<'_, EntityIndex>| {
        let mut attackers = world.get_mut::<RecentAttackers>(victim).unwrap();
        attackers.struck(two, Tick::new(0), &index);
    });
    game.resource_mut::<PassQueue>().push_damage(Damage {
        source: Some(four),
        target: one,
        amount: Num::int(10),
        kind: DamageKind::new(0),
        cause: DamageCause::Effect,
        action: None,
        depth: 0,
        hit: None,
    });
    Mode::tick_inputs(&mut game, &[]);
    // It died in tick 0 with killer 4 of b and one assister of a: the mode set its respawn for the
    // end of tick 0, 1, plus 3 ticks: the start of tick 4.
    let seen = ["count", "kind", "team", "grunts", "path"].map(|name| Mode::field(&game, name));
    let text = |text: &str| StateValue::Text(text.to_owned());
    assert_eq!(
        seen,
        [
            StateValue::Int(1),
            text("grunt"),
            text("b"),
            StateValue::Int(1),
            text("a"),
        ]
    );
    assert_eq!(
        game.get::<Respawn>(victim),
        Some(&Respawn { at: Tick::new(4) })
    );
    for _ in 1..4 {
        Mode::tick_inputs(&mut game, &[]);
        assert!(game.entity(victim).contains::<Dead>());
    }
    Mode::tick_inputs(&mut game, &[]);
    assert!(!game.entity(victim).contains::<Dead>());
    assert_eq!(
        game.get::<Position>(victim),
        Some(&Position::from((0, 0, 0)))
    );
    let life = game.get::<Life>(victim).unwrap();
    assert_eq!(life.current(), Num::int(10));

    // A living unit, and a dead one whose type despawns, cannot respawn.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "tower"))]);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ApiError::RespawnAlive)]
    );
    let tower = Mode::entity_by_id(&game, 0);
    game.entity_mut(tower).insert((Dead, OnDeath::Despawn));
    Mode::tick_inputs(&mut game, &[(0, input("probe", "tower"))]);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ApiError::RespawnDespawns)]
    );
    assert!(game.get_entity(tower).is_err());
}

/// The ticks a pass between two stages ran in, one count each pass.
#[derive(Resource, Debug, Default)]
struct GapRuns(u32);

#[test]
fn a_match_ends_once_and_then_no_stage_and_no_pass_between_them_runs() {
    // A timer counts every tick; inputs end the match.
    let script = r#"
fn on_match_start(ctx) {
    ctx.timer("every", 100, true, ());
    ctx.spawn_group("a", "mid", PathEnd::Start, ["grunt"]);
}

fn on_timer(ctx, name, data) {
    ctx.state.count += 1;
}

fn on_mode_input(ctx, player, name, value) {
    if name == "probe" {
        ctx.end(value);
        ctx.end(value);
    } else if name == "hero" {
        ctx.end(value);
    } else {
        ctx.end(());
    }
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    // A second end in the same call fails the call, which ends nothing; so does a team the mode
    // does not have. The timer fires at the end of ticks 0 and 1.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "a"))]);
    assert_eq!(Mode::failures(&game), [FailureKind::Api(ApiError::Ended)]);
    Mode::tick_inputs(&mut game, &[(0, input("hero", "z"))]);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ApiError::Unknown(NameKind::Team))]
    );
    assert!(game.match_end().is_none());
    assert_eq!(Mode::field(&game, "count"), StateValue::Int(2));

    // A pass in each gap between stages counts its runs: the gap before the first stage and
    // the one after each of the nine.
    game.init_resource::<GapRuns>();
    game.schedule_scope(SimUpdate, |_, schedule| {
        let count = |mut runs: ResMut<'_, GapRuns>| runs.0 += 1;
        schedule.add_systems(count.in_set(SimEdge::Start));
        for stage in SimSet::ALL {
            schedule.add_systems(count.in_set(SimEdge::After(stage)));
        }
    });

    // Team b wins in the Inputs stage of tick 2, so no later stage of tick 2 runs: a's grunt,
    // 1, sent 5 m away, stands where it is, and the timer counts no more. Of the gaps, the one
    // before Inputs runs in tick 2, and the one after it, Inputs' closing set: 2 runs. In tick 3
    // nothing runs, the input to end again included.
    let grunt = Mode::entity_by_id(&game, 1);
    let mut destination = game.get_mut::<Destination>(grunt).unwrap();
    *destination = Destination::to(Some(Position::from((5, 0, 0))));
    let before = Mode::placed_units(&game);
    Mode::tick_inputs(&mut game, &[(0, input("hero", "b"))]);
    let end = MatchEnd::new(Tick::new(2), MatchResult::Won(Team::new(1)));
    assert_eq!(game.match_end(), Some(end));
    Mode::tick_inputs(&mut game, &[(0, input("phase", "draw"))]);
    assert!(Mode::failures(&game).is_empty());
    assert_eq!(game.match_end(), Some(end));
    assert_eq!(Mode::placed_units(&game), before);
    assert_eq!(Mode::field(&game, "count"), StateValue::Int(2));
    assert_eq!(game.resource::<SimTick>().start(), Tick::new(4));
    assert_eq!(game.resource::<GapRuns>().0, 2);

    // The grunt walks, so `keep_in_bounds`, in the gap after Collide, would clamp it back into
    // the bounds; past the end it stays where it is put.
    assert!(game.entity(grunt).contains::<MoveStep>());
    let away = Position::from((10_000, 0, 0));
    *game.get_mut::<Position>(grunt).unwrap() = away;
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(game.get::<Position>(grunt), Some(&away));
    assert_eq!(game.resource::<GapRuns>().0, 2);

    // `end(())` is a draw.
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[(0, input("phase", "draw"))]);
    let draw = MatchEnd::new(Tick::new(0), MatchResult::Draw);
    assert_eq!(game.match_end(), Some(draw));
}
