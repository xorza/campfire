fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, "true", ctx.params.damage);
}
