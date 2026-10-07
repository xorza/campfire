use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_sim::{SimEdge, SimUpdate};

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
    let mut game = Game::new(script, ScriptLimits::ROOMY);
    // Units: the tower 0 of a, the neutral grunt 1 at (0, 0), a's grunts 2 and 3, b's grunt 4. The
    // neutral grunt stands at (3, 0) when b's grunt strikes it for its 10 health, in tick 0; a's
    // grunt 2 struck it in the same tick, within the window of 10 ticks.
    let victim = game.entity(1);
    *game.sim.world.get_mut::<Position>(victim).unwrap() = at(3, 0);
    game.sim.world.insert_resource(AssistWindow(Ticks::new(10)));
    let ids: Vec<_> = game
        .sim
        .world
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, _)| id)
        .collect();
    let (one, two, four) = (ids[1], ids[2], ids[4]);
    game.sim
        .world
        .resource_scope(|world, index: Mut<'_, EntityIndex>| {
            let mut attackers = world.get_mut::<RecentAttackers>(victim).unwrap();
            attackers.record(two, Tick::new(0), &index);
        });
    game.sim
        .world
        .resource_mut::<PassQueue>()
        .push_damage(Damage {
            source: Some(four),
            target: one,
            amount: Num::int(10),
            kind: DamageKind::new(0),
            cause: DamageCause::Effect,
            ability: None,
            depth: 0,
            hit: None,
        });
    game.tick(&[]);
    // It died in tick 0 with killer 4 of b and one assister of a: the mode set its respawn for the
    // end of tick 0, 1, plus 3 ticks: the start of tick 4.
    let seen = ["count", "kind", "team", "grunts", "path"].map(|name| game.field(name));
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
        game.sim.world.get::<Respawn>(victim),
        Some(&Respawn { at: Tick::new(4) })
    );
    for _ in 1..4 {
        game.tick(&[]);
        assert!(game.sim.world.entity(victim).contains::<Dead>());
    }
    game.tick(&[]);
    assert!(!game.sim.world.entity(victim).contains::<Dead>());
    assert_eq!(game.sim.world.get::<Position>(victim), Some(&at(0, 0)));
    let pools = game.sim.world.get::<Pools>(victim).unwrap();
    assert_eq!(pools.current(PoolId::FIRST), Some(Num::int(10)));

    // A living unit, and a dead one whose type despawns, cannot respawn.
    game.tick(&[(0, input("probe", "tower"))]);
    assert_eq!(game.failures(), [FailureKind::Api(ApiError::RespawnAlive)]);
    let tower = game.entity(0);
    game.sim
        .world
        .entity_mut(tower)
        .insert((Dead, OnDeath::Despawn));
    game.tick(&[(0, input("probe", "tower"))]);
    assert_eq!(
        game.failures(),
        [FailureKind::Api(ApiError::RespawnDespawns)]
    );
    assert!(game.sim.world.get_entity(tower).is_err());
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
    let mut game = Game::new(script, ScriptLimits::ROOMY);
    // A second end in the same call fails the call, which ends nothing; so does a team the mode
    // does not have. The timer fires at the end of ticks 0 and 1.
    game.tick(&[(0, input("probe", "a"))]);
    assert_eq!(game.failures(), [FailureKind::Api(ApiError::Ended)]);
    game.tick(&[(0, input("hero", "z"))]);
    assert_eq!(game.failures(), [FailureKind::Api(ApiError::UnknownTeam)]);
    assert!(!game.sim.world.contains_resource::<MatchEnd>());
    assert_eq!(game.field("count"), StateValue::Int(2));

    // A pass in each gap between stages counts its runs: the gap before the first stage and
    // the one after each of the nine.
    game.sim.world.init_resource::<GapRuns>();
    game.sim.world.schedule_scope(SimUpdate, |_, schedule| {
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
    let grunt = game.entity(1);
    let mut destination = game.sim.world.get_mut::<Destination>(grunt).unwrap();
    destination.set(Some(at(5, 0)));
    let before = game.units();
    game.tick(&[(0, input("hero", "b"))]);
    let end = MatchEnd::new(Tick::new(2), MatchResult::Won(Team::new(1)));
    assert_eq!(game.sim.world.get_resource::<MatchEnd>(), Some(&end));
    game.tick(&[(0, input("phase", "draw"))]);
    assert!(game.failures().is_empty());
    assert_eq!(game.sim.world.get_resource::<MatchEnd>(), Some(&end));
    assert_eq!(game.units(), before);
    assert_eq!(game.field("count"), StateValue::Int(2));
    assert_eq!(game.sim.world.resource::<SimTick>().start(), Tick::new(4));
    assert_eq!(game.sim.world.resource::<GapRuns>().0, 2);

    // The grunt walks, so `keep_in_bounds`, in the gap after Collide, would clamp it back into
    // the bounds; past the end it stays where it is put.
    assert!(game.sim.world.entity(grunt).contains::<MoveStep>());
    let away = at(10_000, 0);
    *game.sim.world.get_mut::<Position>(grunt).unwrap() = away;
    game.tick(&[]);
    assert_eq!(game.sim.world.get::<Position>(grunt), Some(&away));
    assert_eq!(game.sim.world.resource::<GapRuns>().0, 2);

    // `end(())` is a draw.
    let mut game = Game::new(script, ScriptLimits::ROOMY);
    game.tick(&[(0, input("phase", "draw"))]);
    let draw = MatchEnd::new(Tick::new(0), MatchResult::Draw);
    assert_eq!(game.sim.world.get_resource::<MatchEnd>(), Some(&draw));
}
