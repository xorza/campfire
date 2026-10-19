fn on_resolve(ctx, caster, target) {
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "slow", ctx.params.slow_ms);
}
