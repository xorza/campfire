use std::num::NonZeroU64;

use campfire_capabilities::Release;
use campfire_script::*;
use campfire_values::EngineEnum;

#[test]
fn the_facts_are_the_names_a_script_uses_on_ctx_and_gives_the_api() {
    let source = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies:avatar") {
        ctx.damage(unit, ctx.p.damage * unit.stat("armor"), "magic");
        if !unit.has_modifier("kindle") {
            ctx.add_modifier(unit, "kindle", 100);
        }
    }
    ctx.restore(caster, "mana", caster.pool("energy") + caster.pool_max("health"));
    for camp in ctx.map.markers("camp") {}
    ctx.grant(caster, "spell", ctx.chosen(0, "spells"));
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
            ("p", value),
            ("damage", call),
            ("p", value),
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
            (NameKind::Filter, "enemies:avatar"),
            (NameKind::DamageKind, "magic"),
            (NameKind::Stat, "armor"),
            (NameKind::Modifier, "kindle"),
            (NameKind::Modifier, "kindle"),
            (NameKind::Pool, "mana"),
            (NameKind::Pool, "energy"),
            (NameKind::Pool, "health"),
            (NameKind::MarkerTag, "camp"),
            (NameKind::SlotKind, "spell"),
            (NameKind::Choice, "spells"),
            (NameKind::Resource, "gold"),
            (NameKind::Team, "north"),
            (NameKind::Path, "mid"),
            (NameKind::Tag, "core"),
            (NameKind::Tag, "slowed"),
            (NameKind::Track, "level"),
        ]
    );
    assert!(!facts.function_pointer);
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
        engine_enum: EngineEnum::PathEnd,
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
fn a_call_with_its_receiver_first_reads_as_the_method_and_a_literal_index_as_data() {
    // Rhai calls `f(x, …)` as `x.f(…)`: with `ctx` first, a call of `ctx`, its names and enums
    // read from the arguments after it; with a handle first, its method; the script's own
    // `helper` neither. A string literal index names data as a property does; a variable cannot
    // be read at load.
    let source = r#"
fn on_resolve(ctx, caster, target) {
    damage(ctx, target, 5, "fire");
    spawn_group(ctx, "a", "mid", "start", []);
    let speed = stat(caster, "speed");
    let width = ctx.p["width"];
    ctx.state["round"] = 1;
    target.state["marks"] += 1;
    let name = "hidden";
    let hidden = ctx.p[name];
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
            ("p", value),
            ("state", value),
            ("p", value)
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
            (NameKind::DamageKind, "fire"),
            (NameKind::Team, "a"),
            (NameKind::Path, "mid"),
            (NameKind::Stat, "speed"),
        ]
    );
    let string = EnumString {
        call: "spawn_group".to_owned(),
        engine_enum: EngineEnum::PathEnd,
    };
    assert_eq!(facts.enum_strings, [string]);
    assert_eq!(facts.params, ["width"]);
    let mut state_fields = facts.state_fields.clone();
    state_fields.sort_unstable();
    assert_eq!(state_fields, ["marks", "round"]);
}

#[test]
fn a_closure_an_anonymous_function_and_fn_make_a_function_pointer() {
    let host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = Release::script_api();
    for (source, pointer) in [
        ("fn on_x(ctx) { let x = 1; let f = || x; }", true),
        ("fn on_x(ctx) { let f = |c| c + 1; }", true),
        (
            r#"fn on_x(ctx) { let f = Fn("h"); f.call(1) } fn h(c) {}"#,
            true,
        ),
        ("fn on_x(ctx, name) { g(Fn(name)) } fn g(c) {}", true),
        ("fn on_x(ctx) { h(1); } fn h(c) {}", false),
    ] {
        let facts = ScriptFacts::read(&host, source, &api).unwrap();
        assert_eq!(facts.function_pointer, pointer, "{source}");
    }
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
