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
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    game.tick(&[(0, input("hero", "hero-x"))]);
    let actor = game.fighter(0, &[]);
    let target = game.fighter(1, &[]);
    let entity = game.sim.entity(target);
    let mut pools = Pools::new([(PoolId::FIRST, Num::int(1000)), (MANA, Num::int(100))]).unwrap();
    pools.take(PoolId::FIRST, Num::int(20));
    pools.take(MANA, Num::int(50));
    game.sim.insert(target, pools);
    // Each role in turn: 3 restored and 5 gold given as its effects apply, then 10 dealt and 4
    // healed in the tick's pass, in the order queued: from 980 to 974, and so on; the pool from
    // 50, 3 a call.
    let gold = |game: &Game| {
        let resources = game.sim.world.resource::<PlayerResources>();
        let gold = resource("gold").unwrap();
        resources.amount(PlayerSlot::new(0), gold)
    };
    for (at, role) in ScriptRole::ALL.into_iter().enumerate() {
        let at = i64::try_from(at).unwrap();
        let before = gold(&game);
        let read = game.probe(probe, role, actor, target).unwrap();
        assert_eq!(gold(&game), before + 5, "{role:?}");
        let read: Array = read.cast();
        let names = |value: &Dynamic| -> Vec<String> {
            let list: Array = value.clone().cast();
            list.into_iter().map(|name| name.to_string()).collect()
        };
        assert_eq!(names(&read[0]), ["a", "b"], "{role:?}");
        assert_eq!(names(&read[1]), ["mid"], "{role:?}");
        assert_eq!(
            (read[2].as_int(), read[3].as_int()),
            (Ok(1), Ok(1)),
            "{role:?}"
        );
        assert_eq!(game.sim.life(target), Num::int(980 - 6 * at), "{role:?}");
        game.tick(&[]);
        assert_eq!(
            game.sim.life(target),
            Num::int(980 - 6 * (at + 1)),
            "{role:?}"
        );
        let pool = game.sim.world.get::<Pools>(entity).unwrap().current(MANA);
        assert_eq!(pool, Some(Num::int(50 + 3 * (at + 1))), "{role:?}");
    }
    // A call given to other roles fails in this one, when it runs.
    let refused = [
        ("ctx.timer(\"late\", 100, false, ())", ScriptRole::Action),
        ("ctx.end(())", ScriptRole::Ai),
        ("ctx.state.phase", ScriptRole::Modifier),
        ("ctx.order_follow_path(unit)", ScriptRole::Mode),
    ];
    for (call, role) in refused {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = game.probe(&source, role, actor, actor).unwrap_err();
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
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    game.tick(&[]);
    let a = game.fighter(0, &[]);
    game.fighter(1, &[]);
    let counts = r#"
fn probe(ctx, unit) {
    ["hostiles", "neutrals", "enemies"].map(|filter| ctx.find(unit, unit.pos, 1, filter).len())
}
"#;
    let count = |game: &mut Game| -> Vec<INT> {
        let counts: Array = game.probe(counts, ScriptRole::Mode, a, a).unwrap().cast();
        counts
            .into_iter()
            .map(|count| count.as_int().unwrap())
            .collect()
    };
    // From a: b's fighter is hostile, the grunt neutral, and both are enemies a may attack.
    assert_eq!(count(&mut game), [1, 1, 2]);
    // A unit's script, as a modifier's, turns the pair hostile; the grunt is a hostile then.
    let turn = r#"fn probe(ctx, unit) { ctx.set_relation("neutral", "a", Relation::Hostile) }"#;
    let turned = game.probe(turn, ScriptRole::Modifier, a, a).unwrap();
    assert!(turned.is_unit());
    assert_eq!(count(&mut game), [2, 0, 2]);
    // A member is its data name as text, the member `named` reads, and equal only to itself.
    let members = r#"fn probe(ctx, unit) {
        [Relation::Hostile.to_string(), Relation::named("neutral") == Relation::Neutral,
         Relation::Hostile != Relation::Friendly, Relation::Hostile == Relation::Neutral]
    }"#;
    let members: Array = game.probe(members, ScriptRole::Mode, a, a).unwrap().cast();
    assert_eq!(members[0].clone().into_string().unwrap(), "hostile");
    let compared = members[1..].iter().map(|member| member.as_bool().unwrap());
    assert_eq!(compared.collect::<Vec<_>>(), [true, true, false]);
    // A text that names no attitude fails `named`, and a team's attitude to itself, or to a
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
            ApiError::UnknownTeam,
        ),
    ] {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = game.probe(&source, ScriptRole::Mode, a, a).unwrap_err();
        assert_eq!(failed.kind(), FailureKind::Api(refused), "{call}: {failed}");
    }
}
