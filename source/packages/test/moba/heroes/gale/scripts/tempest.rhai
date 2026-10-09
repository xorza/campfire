fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {
        ctx.knock_back(unit, caster.pos, ctx.p.knock_back, ctx.p.knock_back_ms);
    }
}

fn on_channel_tick(ctx, caster) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "allies") {
        ctx.heal(unit, ctx.p.heal_per_tick);
    }
}
