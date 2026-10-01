fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "physical");
    ctx.add_modifier(target, "slow", ctx.p.slow_ms);
}
