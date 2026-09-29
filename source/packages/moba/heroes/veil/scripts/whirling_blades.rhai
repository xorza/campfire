fn on_cast(ctx, caster, target) {
    for unit in ctx.enemies_near(caster, caster.pos, ctx.value("radius")) {
        ctx.damage(caster, unit, ctx.value("damage"), "physical");
    }
}
