fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "true");
}
