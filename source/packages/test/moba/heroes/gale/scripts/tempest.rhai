fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {
        ctx.knock_back(unit, caster.pos, ctx.params.knock_back, ctx.params.knock_back_ms);
    }
}

fn on_channel_tick(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "allies") {
        ctx.heal(unit, ctx.params.heal_per_tick);
    }
}
