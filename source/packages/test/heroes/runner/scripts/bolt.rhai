fn on_resolve(ctx, caster, target) {
    ctx.damage(target, "true", ctx.params.damage);
}
