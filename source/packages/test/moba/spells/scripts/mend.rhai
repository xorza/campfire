fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "allies") {
        ctx.heal(unit, ctx.p.heal);
    }
}
