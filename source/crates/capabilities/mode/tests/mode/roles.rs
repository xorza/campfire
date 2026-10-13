use campfire_core::{ScriptList, ScriptValue};
use campfire_sim::internals::SimWorldInternals;

use super::*;

#[test]
fn every_role_reads_the_match_and_deals_damage_heals_and_restores() {
    let probe = r#"
fn probe(ctx, unit) {
    ctx.damage(unit, 10, "true");
    ctx.heal(unit, 4);
    ctx.restore(unit, "mana", 3);
    ctx.add_resource(0, "gold", 5);
    [ctx.teams, ctx.map.paths, ctx.avatars().len(), ctx.units_tagged("avatar").len()]
}
"#;
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    let actor = Mode::stat_grunt(&mut game, Team::new(0), &[]);
    let target = Mode::stat_grunt(&mut game, Team::new(1), &[]);
    let entity = game.unit_entity(target);
    let mut life = Life::full(Num::int(1000));
    life.take(Num::int(20));
    let mut pools = Pools::new([(MANA, Num::int(100))]).unwrap();
    pools.take(MANA, Num::int(50));
    game.insert_parts(target, (life, pools));
    // Each role in turn: 3 restored and 5 gold given as its effects apply, then 10 dealt and 4
    // healed in the tick's pass, in the order queued: from 980 to 974, and so on; the pool from
    // 50, 3 a call.
    let gold = |game: &Match| {
        let resources = game.player_resources(PlayerSlot::new(0));
        resources.unwrap().amount(resource("gold").unwrap())
    };
    for (at, role) in ScriptRole::ALL.into_iter().enumerate() {
        let at = i64::try_from(at).unwrap();
        let before = gold(&game);
        let read = Mode::probe_as(&mut game, probe, role, actor, target).unwrap();
        assert_eq!(gold(&game), before + 5, "{role:?}");
        let read: ScriptList = read.cast();
        let names = |value: &ScriptValue| -> Vec<String> {
            let list: ScriptList = value.clone().cast();
            list.into_iter()
                .map(|name| name.into_text().unwrap().to_string())
                .collect()
        };
        assert_eq!(names(&read[0]), ["a", "b"], "{role:?}");
        assert_eq!(names(&read[1]), ["mid"], "{role:?}");
        assert_eq!(
            (read[2].as_int(), read[3].as_int()),
            (Some(1), Some(1)),
            "{role:?}"
        );
        assert_eq!(
            game.part::<Life>(target).current(),
            Num::int(980 - 6 * at),
            "{role:?}"
        );
        Mode::tick_inputs(&mut game, &[]);
        assert_eq!(
            game.part::<Life>(target).current(),
            Num::int(980 - 6 * (at + 1)),
            "{role:?}"
        );
        let pool = game.get::<Pools>(entity).unwrap().current(MANA);
        assert_eq!(pool, Some(Num::int(50 + 3 * (at + 1))), "{role:?}");
    }
    // A call given to other roles fails in this one, when it runs, whether its body reads the role
    // or not: the binder checks the roles the API declares.
    let refused = [
        ("ctx.timer(\"late\", 100, false, ())", ScriptRole::Action),
        ("ctx.end(())", ScriptRole::Ai),
        ("ctx.state.phase", ScriptRole::Modifier),
        ("ctx.order_follow_path(unit)", ScriptRole::Mode),
        ("ctx.save()", ScriptRole::Action),
        ("ctx.players", ScriptRole::Ai),
        ("ctx.team_of(0)", ScriptRole::Modifier),
        ("ctx.reveal(unit.pos, 1, 100)", ScriptRole::Mode),
        ("ctx.attack_hit(unit)", ScriptRole::Mode),
        ("ctx.origin", ScriptRole::Modifier),
        ("ctx.area(unit.pos)", ScriptRole::Ai),
    ];
    for (call, role) in refused {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = Mode::probe_as(&mut game, &source, role, actor, actor).unwrap_err();
        assert_eq!(
            failed.kind(),
            FailureKind::Api(ApiError::NotForRole),
            "{call} in {role:?}: {failed}"
        );
    }
}

#[test]
fn a_script_turns_a_neutral_pair_hostile_and_filters_follow_it() {
    // Team a and the neutral team regard each other neutral, as the mode declares; b is hostile to
    // both. At the origin: a's and b's fighters, and the map's neutral grunt.
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[]);
    let a = Mode::stat_grunt(&mut game, Team::new(0), &[]);
    Mode::stat_grunt(&mut game, Team::new(1), &[]);
    let counts = r#"
fn probe(ctx, unit) {
    ["hostiles", "neutrals", "enemies"].map(|filter| ctx.find(unit, unit.pos, 1, filter).len())
}
"#;
    let count = |game: &mut Match| -> Vec<i64> {
        let counts: ScriptList = Mode::probe_as(game, counts, ScriptRole::Mode, a, a)
            .unwrap()
            .cast();
        counts
            .into_iter()
            .map(|count| count.as_int().unwrap())
            .collect()
    };
    // From a: b's fighter is hostile, the grunt neutral, and both are enemies a may attack.
    assert_eq!(count(&mut game), [1, 1, 2]);
    // A unit's script, as a modifier's, turns the pair hostile; the grunt is a hostile then.
    let turn = r#"fn probe(ctx, unit) { ctx.set_relation("neutral", "a", Relation::Hostile) }"#;
    let turned = Mode::probe_as(&mut game, turn, ScriptRole::Modifier, a, a).unwrap();
    assert!(turned.is_unit());
    assert_eq!(count(&mut game), [2, 0, 2]);
    // A member is its data name as text, the member `named` reads, and equal only to itself.
    let members = r#"fn probe(ctx, unit) {
        [Relation::Hostile.to_string(), Relation::named("neutral") == Relation::Neutral,
         Relation::Hostile != Relation::Friendly, Relation::Hostile == Relation::Neutral]
    }"#;
    let members: ScriptList = Mode::probe_as(&mut game, members, ScriptRole::Mode, a, a)
        .unwrap()
        .cast();
    assert_eq!(members[0].clone().into_text().unwrap().as_str(), "hostile");
    let compared = members[1..].iter().map(|member| member.as_bool().unwrap());
    assert_eq!(compared.collect::<Vec<_>>(), [true, true, false]);
    // A text that names no relation fails `named`, and a team's relation to itself, or to a
    // team the mode lacks, fails the call.
    for (call, refused) in [
        (
            r#"ctx.set_relation("a", "b", Relation::named("angry"))"#,
            ApiError::UnknownMember(EngineEnum::Relation),
        ),
        (
            r#"ctx.set_relation("b", "b", Relation::Neutral)"#,
            ApiError::SelfRelation,
        ),
        (
            r#"ctx.set_relation("a", "c", Relation::Neutral)"#,
            ApiError::Unknown(NameKind::Team),
        ),
    ] {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = Mode::probe_as(&mut game, &source, ScriptRole::Mode, a, a).unwrap_err();
        assert_eq!(failed.kind(), FailureKind::Api(refused), "{call}: {failed}");
    }
}
