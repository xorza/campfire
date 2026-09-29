fn on_cast(ctx, caster, target) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.slow(target, ctx.p.slow, ctx.p.slow_ms);
}
