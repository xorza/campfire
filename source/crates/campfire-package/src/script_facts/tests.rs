use std::num::NonZeroU64;

use campfire_capabilities::CapabilitySet;
use campfire_script::ScriptHost;

use super::*;

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
    let api = CapabilitySet::bind_script_api(&mut host);
    let ast = host.parse(source).unwrap();
    let facts = ScriptFacts::read(&ast, &api);
    let function = |name: &str, params| Function {
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
    let api = CapabilitySet::bind_script_api(&mut host);
    let facts = ScriptFacts::read(&host.parse(source).unwrap(), &api);
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
fn a_closure_an_anonymous_function_and_fn_make_a_function_pointer() {
    let host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let api = CapabilitySet::script_api();
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
        let facts = ScriptFacts::read(&host.parse(source).unwrap(), &api);
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
    let api = CapabilitySet::script_api();
    for (source, misuse) in cases {
        let facts = ScriptFacts::read(&host.parse(source).unwrap(), &api);
        assert_eq!(facts.ctx_misuse, misuse, "{source}");
    }
    let facts = ScriptFacts::read(&host.parse("fn on_x(c, ctx) {}").unwrap(), &api);
    assert!(!facts.functions[0].ctx_first);
}
