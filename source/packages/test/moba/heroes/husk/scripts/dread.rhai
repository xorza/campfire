fn on_interval(ctx, m) {
    for unit in ctx.find(m.carrier, m.carrier.position, ctx.params.radius, "enemies") {
        ctx.damage(unit, "magic", unit.pool_max("health") * ctx.params.share);
    }
}
