fn on_cast(ctx, caster, target) {
    for unit in ctx.enemies_near(caster, caster.pos, ctx.value("radius")) {
        ctx.knock_back(unit, caster.pos, ctx.value("knock_back"), ctx.value("knock_back_ms"));
    }
}

fn on_channel_tick(ctx, caster) {
    for unit in ctx.allies_near(caster, caster.pos, ctx.value("radius")) {
        ctx.heal(unit, ctx.value("heal_per_tick"));
    }
}
