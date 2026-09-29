fn on_cast(ctx, caster, target) {
    ctx.add_modifier(caster, "unbound");
}

fn on_takedown(ctx, m, victim) {
    if victim.is_hero {
        ctx.reduce_cooldowns(m.carrier, ctx.value("cooldown_cut"));
    }
}
