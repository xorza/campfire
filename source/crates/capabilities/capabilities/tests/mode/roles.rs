use campfire_script::{CapabilityRefusal, ScriptList, ScriptValue};
use campfire_sim::internals::SimWorldInternals;
use campfire_values::Named;

use super::*;

/// The test mode with a `dummy`, a grunt of 1000 health, and the mode's own `strike`, whose
/// call an action's role makes.
fn roles_set() -> PackageSet {
    let mut set = mode_packages(SCRIPT);
    set.units.push_str(&stat_type("dummy", &[]));
    set.data
        .push_str("\n[actions.strike]\ntargeting = \"none\"\n");
    set
}

#[test]
fn every_role_reads_the_match_and_all_but_the_ai_deal_damage_heal_and_restore() {
    let changes = r#"
    ctx.damage(unit, "true", 10);
    ctx.heal(unit, 4);
    ctx.restore(unit, "mana", 3);
    ctx.add_resource(ctx.players[0], "gold", 5);"#;
    let reads = r#"[ctx.teams, ctx.map.paths, ctx.avatars().len(), ctx.units_tagged("avatar").len(),
        ctx.players.len()]"#;
    let mut game = started(&roles_set(), ScriptLimits::ROOMY).unwrap();
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    let actor = spawn_of(&mut game, "dummy", Team::new(0));
    let target = spawn_of(&mut game, "dummy", Team::new(1));
    let entity = game.unit_entity(target);
    let mut life = Life::full(Num::int(1000));
    life.take(Num::int(20));
    let mut pools = Pools::new([(MANA, Num::int(100))]).unwrap();
    pools.take(MANA, Num::int(50));
    game.insert_parts(target, (life, pools));
    // Each role in turn: 3 restored and 5 gold given as its effects apply, then 10 dealt and 4
    // healed in the tick's pass, in the order queued: from 980 to 974, and so on; the pool from
    // 50, 3 a call. The AI's call only reads, as an AI reads and orders, and changes nothing.
    let gold = |game: &Match| {
        let resources = game.player_resources(Seat::new(0));
        resources.unwrap().amount(resource("gold").unwrap())
    };
    let mut at = 0;
    for role in ScriptRole::ALL {
        let changes = if role == ScriptRole::Ai { "" } else { changes };
        let probe = format!("fn probe(ctx, unit) {{ {changes} {reads} }}");
        let before = gold(&game);
        let read = Mode::probe_as(&mut game, &probe, role, actor, target).unwrap();
        let after = at + i64::from(!changes.is_empty());
        assert_eq!(gold(&game), before + 5 * (after - at), "{role:?}");
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
            (read[2].as_int(), read[3].as_int(), read[4].as_int()),
            (Some(1), Some(1), Some(3)),
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
            Num::int(980 - 6 * after),
            "{role:?}"
        );
        let pool = game.get::<Pools>(entity).unwrap().current(MANA);
        assert_eq!(pool, Some(Num::int(50 + 3 * after)), "{role:?}");
        at = after;
    }
    // A call given to other roles fails in this one, when it runs, whether its body reads the role
    // or not: the binder checks the roles the API declares.
    let refused = [
        ("ctx.after(\"late\", 100)", ScriptRole::Action),
        ("ctx.draw()", ScriptRole::Ai),
        ("ctx.state.phase", ScriptRole::Modifier),
        ("ctx.save()", ScriptRole::Action),
        ("ctx.attack_hit(unit)", ScriptRole::Mode),
        ("ctx.origin", ScriptRole::Modifier),
        (r#"ctx.damage(unit, "true", 1)"#, ScriptRole::Ai),
        ("ctx.heal(unit, 1)", ScriptRole::Ai),
        (r#"ctx.restore(unit, "mana", 1)"#, ScriptRole::Ai),
        (
            r#"ctx.add_resource(ctx.players[0], "gold", 1)"#,
            ScriptRole::Ai,
        ),
        (r#"ctx.reveal(unit.position, 1, 100, "a")"#, ScriptRole::Ai),
        ("ctx.players[0].state.kills = 1", ScriptRole::Ai),
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
    // A reveal that names its team serves every role that changes the match, the mode's among
    // them.
    for role in ScriptRole::ALL
        .into_iter()
        .filter(|role| *role != ScriptRole::Ai)
    {
        let source = r#"fn probe(ctx, unit) { ctx.reveal(unit.position, 1, 100, "a") }"#;
        let revealed = Mode::probe_as(&mut game, source, role, actor, actor);
        assert!(revealed.is_ok(), "{role:?}: {revealed:?}");
    }
    // A call of a capability the match does not declare, `orders` or `areas`, is not bound,
    // even in a role it serves: the match binds the API of its declared capabilities alone.
    let unbound = [
        ("ctx.order_follow_path()", ScriptRole::Ai),
        ("ctx.land(unit.position)", ScriptRole::Action),
    ];
    for (call, role) in unbound {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = Mode::probe_as(&mut game, &source, role, actor, actor).unwrap_err();
        assert_eq!(
            failed.kind(),
            FailureKind::Runtime,
            "{call} in {role:?}: {failed}"
        );
    }
}

#[test]
fn a_script_turns_a_neutral_pair_hostile_and_filters_follow_it() {
    // Team a and the neutral team regard each other neutral, as the mode declares; b is hostile to
    // both. At the origin: a's and b's fighters, and the map's neutral grunt.
    let mut game = started(&roles_set(), ScriptLimits::ROOMY).unwrap();
    Mode::tick_inputs(&mut game, &[]);
    let a = spawn_of(&mut game, "dummy", Team::new(0));
    spawn_of(&mut game, "dummy", Team::new(1));
    let counts = r#"
fn probe(ctx, unit) {
    let counts = [];
    for filter in ["hostiles", "neutrals", "enemies"] {
        counts.push(ctx.find(unit, unit.position, 1, filter).len());
    }
    counts.push(ctx.relation("a", "neutral") == Relation::Hostile);
    counts.push(ctx.relation("neutral", "b") == Relation::Hostile);
    counts.push(ctx.enemies("a"));
    counts
}
"#;
    // The counts of each filter, whether a regards the neutral team hostile and the neutral
    // team b, and the playing teams a may attack, b alone, as the neutral team does not play.
    let count = |game: &mut Match| -> (Vec<i64>, [bool; 2], Vec<String>) {
        let read: ScriptList = Mode::probe_as(game, counts, ScriptRole::Mode, a, a)
            .unwrap()
            .cast();
        let counts = read[..3].iter().map(|count| count.as_int().unwrap());
        let hostile = [3, 4].map(|at| read[at].as_bool().unwrap());
        let enemies: ScriptList = read[5].clone().cast();
        let enemies = enemies
            .into_iter()
            .map(|team| team.into_text().unwrap().to_string());
        (counts.collect(), hostile, enemies.collect())
    };
    // From a: b's fighter is hostile, the grunt neutral, and both are enemies a may attack.
    assert_eq!(
        count(&mut game),
        (vec![1, 1, 2], [false, true], vec!["b".to_owned()])
    );
    // A unit's script, as a modifier's, turns the pair hostile; the grunt is a hostile then.
    let turn = r#"fn probe(ctx, unit) { ctx.set_relation("neutral", "a", Relation::Hostile) }"#;
    let turned = Mode::probe_as(&mut game, turn, ScriptRole::Modifier, a, a).unwrap();
    assert!(turned.is_unit());
    assert_eq!(
        count(&mut game),
        (vec![2, 0, 2], [true, true], vec!["b".to_owned()])
    );
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
            ModeRefusal::SelfRelation.refused(),
        ),
        (
            r#"ctx.set_relation("a", "c", Relation::Neutral)"#,
            ApiError::Unknown(Team::NAMES),
        ),
    ] {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = Mode::probe_as(&mut game, &source, ScriptRole::Mode, a, a).unwrap_err();
        assert_eq!(failed.kind(), FailureKind::Api(refused), "{call}: {failed}");
    }
}

#[test]
fn the_modes_damage_comes_from_the_source_it_names_else_from_none() {
    // The mode's call has no acting unit: its damage of the target comes from none, and with the
    // actor named, from the actor, whom the target then lists among its recent attackers.
    let mut game = started(&roles_set(), ScriptLimits::ROOMY).unwrap();
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    let actor = spawn_of(&mut game, "dummy", Team::new(0));
    let target = spawn_of(&mut game, "dummy", Team::new(1));
    let attackers = |game: &mut Match, call: &str| -> i64 {
        let deal = format!(
            r#"fn probe(ctx, unit) {{ for other in ctx.find(unit, unit.position, 100, "enemies") {{ {call} }} }}"#
        );
        Mode::probe_as(game, &deal, ScriptRole::Mode, actor, actor).unwrap();
        Mode::tick_inputs(game, &[]);
        let read = "fn probe(ctx, unit) { unit.recent_attackers(1000).len() }";
        let read = Mode::probe_as(game, read, ScriptRole::Mode, actor, target).unwrap();
        read.as_int().unwrap()
    };
    assert_eq!(attackers(&mut game, r#"ctx.damage(other, "true", 1)"#), 0);
    assert_eq!(
        attackers(&mut game, r#"ctx.damage(other, "true", 1, unit)"#),
        1
    );
}
