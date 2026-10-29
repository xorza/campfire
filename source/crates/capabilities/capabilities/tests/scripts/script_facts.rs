use std::num::NonZeroU64;

use campfire_actions::SlotKind;
use campfire_capabilities::Release;
use campfire_mode::{ChoiceId, MarkerTag, TimerId};
use campfire_names::Filter;
use campfire_navigation::PathEnd;
use campfire_script::internals::Undefined;
use campfire_script::*;
use campfire_units::{PoolId, StatId};
use campfire_values::{
    ActionId, DamageKind, ModifierId, Named, PathId, ResourceId, ScriptEnum, Tag, Team, TrackId,
    UnitType,
};

#[test]
fn the_facts_are_the_names_a_script_uses_on_ctx_and_gives_the_api() {
    let source = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies:avatar") {
        ctx.damage(unit, "magic", ctx.params.damage * unit.stat("armor"));
        if !unit.has_modifier("kindle") {
            ctx.add_modifier(unit, "kindle", 100);
        }
    }
    ctx.restore(caster, "mana", caster.pool("energy") + caster.pool_max("health"));
    for camp in ctx.map.markers("camp") {}
    ctx.grant(caster, "spell", ctx.chosen(caster.owner, "spells"));
    ctx.add_resource(caster.owner, "gold", 5);
    ctx.spawn_group("north", "mid", PathEnd::Start, ctx.units_tagged("core"));
    if caster.has_tag("slowed") { ctx.add_xp(caster, "level", 1); }
    let first = ctx.teams[0];
    caster.state.mark = target.state.hits;
    ctx.state.phase = ctx.nearest_visible(caster, 5, first);
    helper(ctx, caster.params.gold);
}

fn helper(ctx, gold) {}
"#;
    let mut host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::bind_script_api(&mut host);
    let facts = ScriptFacts::read(&host, source, &api).unwrap();
    let function = |name: &str, params| ScriptFunction {
        name: name.to_owned(),
        params,
        ctx_first: true,
    };
    assert_eq!(
        facts.functions,
        [function("helper", 2), function("on_resolve", 3)]
    );
    assert_eq!(facts.ctx_misuse, None);
    let names: Vec<_> = facts
        .ctx_names
        .iter()
        .map(|used| (used.name.as_str(), used.kind))
        .collect();
    let (call, value) = (MemberKind::Call, MemberKind::Value);
    assert_eq!(
        names,
        [
            ("find", call),
            ("params", value),
            ("damage", call),
            ("params", value),
            ("add_modifier", call),
            ("restore", call),
            ("map", value),
            ("grant", call),
            ("chosen", call),
            ("add_resource", call),
            ("spawn_group", call),
            ("units_tagged", call),
            ("add_xp", call),
            ("teams", value),
            ("state", value),
            ("nearest_visible", call),
        ]
    );
    assert_eq!(facts.params, ["radius", "damage"]);
    let mut state_fields = facts.state_fields.clone();
    state_fields.sort_unstable();
    assert_eq!(state_fields, ["hits", "mark", "phase"]);
    // A literal counts; a variable, as the filter `first`, cannot be read at load.
    let given: Vec<_> = facts
        .names
        .iter()
        .map(|named| (named.kind, named.name.as_str()))
        .collect();
    assert_eq!(
        given,
        [
            (Filter::NAMES, "enemies:avatar"),
            (DamageKind::NAMES, "magic"),
            (StatId::NAMES, "armor"),
            (ModifierId::NAMES, "kindle"),
            (ModifierId::NAMES, "kindle"),
            (PoolId::NAMES, "mana"),
            (PoolId::NAMES, "energy"),
            (PoolId::NAMES, "health"),
            (MarkerTag::NAMES, "camp"),
            (SlotKind::NAMES, "spell"),
            (ChoiceId::NAMES, "spells"),
            (ResourceId::NAMES, "gold"),
            (Team::NAMES, "north"),
            (PathId::NAMES, "mid"),
            (Tag::NAMES, "core"),
            (Tag::NAMES, "slowed"),
            (TrackId::NAMES, "level"),
        ]
    );
}

#[test]
fn a_string_literal_for_an_enum_and_each_module_path_are_facts() {
    // The string literal `spawn_group`'s end is given counts, the member from `named` does not;
    // a member and a function of an enum's module count as paths.
    let source = r#"
fn on_x(ctx, unit) {
    ctx.set_relation("a", "b", Relation::Hostile);
    ctx.spawn_group("a", "mid", "start", []);
    ctx.spawn_group("a", "mid", PathEnd::named(unit.team), []);
}
"#;
    let mut host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::bind_script_api(&mut host);
    let facts = ScriptFacts::read(&host, source, &api).unwrap();
    let string = EnumString {
        call: "spawn_group".to_owned(),
        engine_enum: <PathEnd as ScriptEnum>::ENUM,
    };
    assert_eq!(facts.enum_strings, [string]);
    let path = |module: &str, name: &str, kind| EnumPath {
        module: module.to_owned(),
        name: name.to_owned(),
        kind,
    };
    assert_eq!(
        facts.enum_paths,
        [
            path("Relation", "Hostile", MemberKind::Value),
            path("PathEnd", "named", MemberKind::Call)
        ]
    );
}

#[test]
fn a_literal_compared_with_a_returned_name_is_a_fact_of_its_kind() {
    // Either side of `==` and `!=`, each literal of a list written in place after `in`, and the
    // last link of a chain count. A name compared with a variable, a field that
    // returns no name, a name after `.p` and a list held in a variable do not; nor does a
    // `switch`'s literal, as its member counts instead.
    let source = r#"
fn on_damage_taken(ctx, m, d) {
    if d.kind == "true" || "pure" != d.kind {}
    if m.carrier.team in ["north", "south"] {}
    if ctx.players[0].team == "north" || ctx.unit(0).path == "mid" {}
    if d.action == "dusk_mark" {}
    let kinds = ["magic"];
    if d.kind in kinds || d.kind == m.state.kind || d.amount == "a" || ctx.params.team == "x" {}
    switch m.carrier.unit_type { "grunt" => {} }
}
"#;
    let mut host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::bind_script_api(&mut host);
    let facts = ScriptFacts::read(&host, source, &api).unwrap();
    let compared = |member: &str, kind, name: &str| ComparedName {
        member: member.to_owned(),
        kind,
        name: name.to_owned(),
    };
    let (damage, team) = (DamageKind::NAMES, Team::NAMES);
    assert_eq!(
        facts.compared,
        [
            compared("kind", damage, "true"),
            compared("kind", damage, "pure"),
            compared("team", team, "north"),
            compared("team", team, "south"),
            compared("team", team, "north"),
            compared("path", PathId::NAMES, "mid"),
            compared("action", ActionId::NAMES, "dusk_mark"),
        ]
    );
    assert_eq!(facts.switched, ["unit_type"]);

    // A list of names counts each literal found in it, by `in` or `contains`, and its item, by
    // an index or as a loop's variable; so does a hook's parameter of names, `on_timer`'s.
    let lists = r#"
fn on_timer(ctx, name, data) {
    if name == "wave" || "west" in ctx.teams || ctx.teams.contains("east") {}
    if ctx.teams[0] == "west" || "grunt" in ctx.unit(0).queue {}
    for team in ctx.enemies("west") {
        if team == "east" {}
    }
    for (path, at) in ctx.map.paths {
        if path == "mid" || at == 1 {}
    }
}
"#;
    let facts = ScriptFacts::read(&host, lists, &api).unwrap();
    assert_eq!(
        facts.compared,
        [
            compared("name", TimerId::NAMES, "wave"),
            compared("teams", team, "west"),
            compared("teams", team, "east"),
            compared("teams", team, "west"),
            compared("queue", UnitType::NAMES, "grunt"),
            compared("team", team, "east"),
            compared("path", PathId::NAMES, "mid"),
        ]
    );
    // A variable its function binds by `let` or writes, anywhere, a parameter of no hook, and
    // a loop's counter, the check does not follow.
    let unfollowed = r#"
fn on_timer(ctx, name, data) {
    name = "other";
    if name == "wave" {}
    for team in ctx.teams {
        let team = "north";
        if team == "south" {}
    }
}
fn helper(ctx, name) {
    if name == "wave" {}
}
"#;
    let facts = ScriptFacts::read(&host, unfollowed, &api).unwrap();
    assert_eq!(facts.compared, []);
}

#[test]
fn a_call_with_its_receiver_first_reads_as_the_method_and_a_literal_index_as_data() {
    // Rhai calls `f(x, …)` as `x.f(…)`: with `ctx` first, a call of `ctx`, its names and enums
    // read from the arguments after it; with a handle first, its method; the script's own
    // `helper` neither. A string literal index names data as a property does; a variable cannot
    // be read at load.
    let source = r#"
fn on_resolve(ctx, caster, target) {
    damage(ctx, target, "fire", 5);
    spawn_group(ctx, "a", "mid", "start", []);
    let speed = stat(caster, "speed");
    let width = ctx.params["width"];
    ctx.state["round"] = 1;
    target.state["marks"] += 1;
    let name = "hidden";
    let hidden = ctx.params[name];
    helper(ctx, caster);
}

fn helper(ctx, unit) {}
"#;
    let mut host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::bind_script_api(&mut host);
    let facts = ScriptFacts::read(&host, source, &api).unwrap();
    assert_eq!(facts.ctx_misuse, None);
    let names: Vec<_> = facts
        .ctx_names
        .iter()
        .map(|used| (used.name.as_str(), used.kind))
        .collect();
    let (call, value) = (MemberKind::Call, MemberKind::Value);
    assert_eq!(
        names,
        [
            ("damage", call),
            ("spawn_group", call),
            ("params", value),
            ("state", value),
            ("params", value)
        ]
    );
    let members: Vec<_> = facts
        .members
        .iter()
        .map(|used| (used.name.as_str(), used.kind))
        .collect();
    assert_eq!(
        members,
        [("stat", MemberKind::Method), ("state", MemberKind::Field)]
    );
    let given: Vec<_> = facts
        .names
        .iter()
        .map(|named| (named.kind, named.name.as_str()))
        .collect();
    assert_eq!(
        given,
        [
            (DamageKind::NAMES, "fire"),
            (Team::NAMES, "a"),
            (PathId::NAMES, "mid"),
            (StatId::NAMES, "speed"),
        ]
    );
    let string = EnumString {
        call: "spawn_group".to_owned(),
        engine_enum: <PathEnd as ScriptEnum>::ENUM,
    };
    assert_eq!(facts.enum_strings, [string]);
    assert_eq!(facts.params, ["width"]);
    let mut state_fields = facts.state_fields.clone();
    state_fields.sort_unstable();
    assert_eq!(state_fields, ["marks", "round"]);
}

#[test]
fn a_closure_fails_to_compile_and_a_function_pointer_reads_as_its_calls() {
    // With no closures, a function that captures a variable names one its scope lacks; an
    // anonymous function that captures nothing is one more function of the script, and `Fn`
    // and a call through a pointer are calls of no namespace.
    let host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::script_api();
    let error = ScriptFacts::read(&host, "fn on_x(ctx) { let x = 1; let f = || x; }", &api);
    let error = error.unwrap_err();
    assert_eq!(error.undefined(), Some(Undefined::Variable("x")));
    for source in [
        "fn on_x(ctx) { let f = |c| c + 1; }",
        r#"fn on_x(ctx) { let f = Fn("h"); f.call(1) } fn h(c) {}"#,
        "fn on_x(ctx, name) { g(Fn(name)) } fn g(c) {}",
    ] {
        let facts = ScriptFacts::read(&host, source, &api).unwrap();
        assert_eq!(facts.ctx_misuse, None, "{source}");
        assert!(
            facts
                .functions
                .iter()
                .any(|function| function.name == "on_x")
        );
    }
    let anonymous = ScriptFacts::read(&host, "fn on_x(ctx) { let f = |c| c + 1; }", &api).unwrap();
    assert_eq!(anonymous.functions.len(), 2);
}

#[test]
fn every_use_of_ctx_but_a_name_on_it_or_a_call_argument_breaks_the_convention() {
    let renamed = |function: &str| {
        Some(CtxMisuse::Renamed {
            function: function.to_owned(),
        })
    };
    let cases = [
        ("fn on_x(ctx) { ctx.find(1); api(ctx, 2); }", None),
        ("fn on_x(ctx) { let c = ctx; }", Some(CtxMisuse::Stray)),
        (
            "fn on_x(ctx) { let c = 0; c = ctx; }",
            Some(CtxMisuse::Stray),
        ),
        ("fn on_x(ctx) { return ctx; }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { [ctx] }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { #{ c: ctx } }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { ctx[0] }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { ctx == 1 }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { h(1, ctx); } fn h(a, c) {}", renamed("h")),
        ("fn on_x(ctx) { 1.h(ctx); } fn h(c) {}", renamed("h")),
        // `ctx` past the first argument of the engine's call, or given a method of the engine's,
        // is no call of `ctx`.
        ("fn on_x(ctx) { api(1, ctx); }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { 1.api(ctx); }", Some(CtxMisuse::Stray)),
        ("fn on_x(ctx) { h(ctx); } fn h(ctx) { ctx.find(1); }", None),
        ("fn on_x(ctx) { let ctx = 1; }", Some(CtxMisuse::Bound)),
        ("fn on_x(ctx) { const ctx = 1; }", Some(CtxMisuse::Bound)),
        // A loop with an empty body is optimized away, so these loops have a body.
        (
            "fn on_x(c) { for ctx in [1] { c.f(1); } }",
            Some(CtxMisuse::Bound),
        ),
        (
            "fn on_x(c) { for (x, ctx) in [1] { c.f(1); } }",
            Some(CtxMisuse::Bound),
        ),
    ];
    let host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::script_api();
    for (source, misuse) in cases {
        let facts = ScriptFacts::read(&host, source, &api).unwrap();
        assert_eq!(facts.ctx_misuse, misuse, "{source}");
    }
    let facts = ScriptFacts::read(&host, "fn on_x(c, ctx) {}", &api).unwrap();
    assert!(!facts.functions[0].ctx_first);
}
