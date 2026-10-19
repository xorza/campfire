fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {
        ctx.damage(unit, "physical", ctx.params.damage);
    }
}
