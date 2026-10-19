fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, "true", ctx.params.damage);
    ctx.add_modifier(target, "slow", ctx.params.slow_ms);
}
