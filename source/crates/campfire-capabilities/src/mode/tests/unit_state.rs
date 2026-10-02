use super::*;
use crate::units::unit_state::UnitState;

#[test]
fn a_call_reads_its_unit_state_writes_back_and_the_next_call_of_the_stage_reads_them() {
    let script = r#"
fn on_mode_input(ctx, player, name, value) {
    if value == "spawn" {
        ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].pos);
        return;
    }
    let grunt = ctx.units_tagged("grunt")[0];
    if value == "write" {
        ctx.state.seen = grunt.state.hits;
        grunt.state.hits = 5;
        grunt.state.hits += 1;
        ctx.state.count = grunt.state.hits;
    } else if value == "read" {
        ctx.state.inputs = grunt.state.hits;
    } else if value == "fail" {
        grunt.state.hits = 9;
        throw "the call fails";
    } else if value == "unknown" {
        grunt.state.hp = 1;
    } else if value == "wrong" {
        grunt.state.hits = "many";
    }
}
"#;
    let mut game = Game::new(script, ScriptLimits::ROOMY);
    game.tick(&[(1, input("probe", "spawn"))]);
    let grunt = |game: &Game| {
        let world = &game.sim.world;
        let grunt_type = world.non_send::<View>().unit_type_named("grunt").unwrap();
        let grunts = world
            .resource::<EntityIndex>()
            .iter()
            .filter(|&(_, entity)| world.get::<UnitType>(entity) == Some(&grunt_type));
        let (_, entity) = grunts.last().unwrap();
        world.get::<UnitState>(entity).unwrap().values().to_vec()
    };
    // It spawns at its type's defaults: 2, and an empty text.
    let at = |hits: i64| vec![StateValue::Int(hits), StateValue::Text(String::new())];
    assert_eq!(grunt(&game), at(2));
    // In one tick, the first call reads 2, writes 5 and then 6, and reads 6 back; the second
    // reads 6, as the first call's writes applied when it ended.
    game.tick(&[(1, input("probe", "write")), (1, input("probe", "read"))]);
    assert_eq!(game.failures(), []);
    let [_, seen, count, inputs] = game.state();
    assert_eq!([seen, count, inputs], [2, 6, 6].map(StateValue::Int));
    assert_eq!(grunt(&game), at(6));
    // A call that fails writes nothing; a field the type does not declare, and a value of
    // another type, fail the call.
    game.tick(&[
        (1, input("probe", "fail")),
        (1, input("probe", "unknown")),
        (1, input("probe", "wrong")),
    ]);
    let refused = [ApiError::UnknownState, ApiError::WrongStateType].map(FailureKind::Api);
    assert_eq!(game.failures()[1..], refused);
    assert!(matches!(game.failures()[0], FailureKind::Raised(None)));
    assert_eq!(grunt(&game), at(6));
    // A snapshot holds the values: a fresh match it restores into holds them too.
    let mut fresh = Game::new(script, ScriptLimits::ROOMY);
    let placed: Vec<Entity> = fresh
        .sim
        .world
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, entity)| entity)
        .collect();
    for entity in placed {
        fresh.sim.world.despawn(entity);
    }
    game.sim.restore_into(&mut fresh.sim);
    assert_eq!(grunt(&fresh), at(6));
}
