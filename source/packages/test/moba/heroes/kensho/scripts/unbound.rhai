fn on_resolve(ctx, caster, target) {
    ctx.add_modifier(caster, "unbound");
}

fn on_takedown(ctx, m, victim) {
    if victim.is_avatar {
        ctx.scale_cooldowns(m.carrier, "basic", ctx.params.cooldown_cut);
    }
}
