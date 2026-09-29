fn on_cast(ctx, caster, target) {
    for unit in ctx.allies_near(caster, caster.pos, ctx.value("radius")) {
        ctx.heal(unit, ctx.value("heal"));
    }
}
