fn on_cast(ctx, caster, target) {
    ctx.add_modifier(target, "scorched");
}

fn on_interval(ctx, m) {
    ctx.damage(m.source, m.carrier, ctx.value("damage_per_second"), "true");
}
