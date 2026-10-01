fn on_resolve(ctx, caster, target) {
    ctx.add_modifier(caster, "unbound");
}

fn on_takedown(ctx, m, victim) {
    if victim.is_avatar {
        ctx.reduce_cooldowns(m.carrier, ctx.p.cooldown_cut);
    }
}
