fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {
        ctx.damage(unit, "magic", ctx.params.damage);
        ctx.add_modifier(unit, "entombed");
    }
}
