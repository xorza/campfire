fn on_interval(ctx, m) {
    for unit in ctx.find(m.carrier, m.carrier.pos, ctx.p.radius, "enemies") {
        ctx.damage(unit, unit.max_health * ctx.p.share, "magic");
    }
}
