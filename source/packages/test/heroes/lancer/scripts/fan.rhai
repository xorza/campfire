fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "true");
    ctx.add_modifier(target, "slow", ctx.p.slow_ms);
}
