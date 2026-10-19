use campfire_units::{SyncOwner, UnitState};

use super::*;

#[test]
fn a_call_reads_its_unit_state_writes_back_and_the_next_call_of_the_stage_reads_them() {
    let script = r#"
fn on_timer(ctx, name, data) {
    ctx.state.kind = data.state.mark;
    ctx.state.seen = data.state.hits;
}

fn on_input(ctx, player, name, value) {
    if value == "spawn" {
        ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].pos);
        return;
    }
    if value == "spawn_marked" {
        let id = ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].pos);
        id.state.mark = "new";
        if id.state.mark != "new" || id.state.hits != 2 {
            throw "a new unit reads its defaults but what the call wrote";
        }
        ctx.after("once", 100, id);
        return;
    }
    if value == "spawn_read" {
        let id = ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].pos);
        id.pos;
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
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[(1, input("probe", "spawn"))]);
    let grunt = |game: &Match| {
        let grunt_type = game
            .resource::<MatchNames>()
            .named((TypeScope::Mode, "grunt"))
            .unwrap();
        let grunts = game
            .resource::<EntityIndex>()
            .iter()
            .filter(|&(_, entity)| game.get::<UnitType>(entity) == Some(&grunt_type));
        let (_, entity) = grunts.last().unwrap();
        let owner = game.get::<UnitState<SyncOwner>>(entity).unwrap().values();
        let all = game.get::<UnitState<SyncAll>>(entity).unwrap().values();
        (owner.to_vec(), all.to_vec())
    };
    // It spawns at its type's defaults: 2, which its owner's class holds, and an empty text,
    // which the class of all holds.
    let at = |hits: i64| {
        (
            vec![StateValue::Int(hits)],
            vec![StateValue::Text("".into())],
        )
    };
    assert_eq!(grunt(&game), at(2));
    // In one tick, the first call reads 2, writes 5 and then 6, and reads 6 back; the second
    // reads 6, as the first call's writes applied when it ended.
    Mode::tick_inputs(
        &mut game,
        &[(1, input("probe", "write")), (1, input("probe", "read"))],
    );
    assert_eq!(Mode::failures(&game), []);
    let [_, seen, count, inputs] = Mode::tracked_state(&game);
    assert_eq!([seen, count, inputs], [2, 6, 6].map(StateValue::Int));
    assert_eq!(grunt(&game), at(6));
    // A call that fails writes nothing; a field the type does not declare, and a value of
    // another type, fail the call.
    Mode::tick_inputs(
        &mut game,
        &[
            (1, input("probe", "fail")),
            (1, input("probe", "unknown")),
            (1, input("probe", "wrong")),
        ],
    );
    let refused = [ApiError::UnknownState, ApiError::WrongStateType].map(FailureKind::Api);
    assert_eq!(Mode::failures(&game)[1..], refused);
    assert!(matches!(
        Mode::failures(&game)[0],
        FailureKind::Raised(None)
    ));
    assert_eq!(grunt(&game), at(6));
    // A snapshot holds the values: a fresh match it restores into holds them too.
    let mut fresh = mode_match(script, ScriptLimits::ROOMY);
    let placed: Vec<Entity> = fresh
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, entity)| entity)
        .collect();
    for entity in placed {
        fresh.despawn(entity);
    }
    Release::restore_into(&game, &mut fresh);
    assert_eq!(grunt(&fresh), at(6));
    // A unit a call spawns is a handle whose state the call writes and reads back, at its type's
    // defaults but what it wrote; the unit spawns with what it wrote. The handle is a unit's,
    // which a timer's data holds, and the timer's call reads it spawned.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "spawn_marked"))]);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(Mode::failures(&game), []);
    let marked = (
        vec![StateValue::Int(2)],
        vec![StateValue::Text("new".into())],
    );
    assert_eq!(grunt(&game), marked);
    assert_eq!(Mode::field(&game, "kind"), StateValue::Text("new".into()));
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(2));
    // Its other fields have no values until it spawns: a read of one fails the call.
    Mode::tick_inputs(&mut game, &[(1, input("probe", "spawn_read"))]);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ApiError::Unspawned)]
    );
}
