fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies") {
        ctx.damage(unit, "true", ctx.params.damage);
    }
}
