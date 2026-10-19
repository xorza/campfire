fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "allies") {
        ctx.heal(unit, ctx.params.heal);
    }
}
