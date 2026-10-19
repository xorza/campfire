fn on_resolve(ctx, caster, target) {
    ctx.add_modifier(target, "scorched");
}

fn on_interval(ctx, m) {
    ctx.damage(m.carrier, "true", ctx.params.damage_per_second);
}
