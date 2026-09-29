fn on_toggle(ctx, caster, on) {
    if on {
        ctx.add_modifier(caster, "dread");
    } else {
        ctx.remove_modifier(caster, "dread");
    }
}

fn on_interval(ctx, m) {
    for unit in ctx.enemies_near(m.carrier, m.carrier.pos, ctx.value("radius")) {
        ctx.damage(m.carrier, unit, unit.max_health * ctx.value("share"), "magic");
    }
}
