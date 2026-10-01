fn on_resolve(ctx, caster, target) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "slow", ctx.p.slow_ms);
}
