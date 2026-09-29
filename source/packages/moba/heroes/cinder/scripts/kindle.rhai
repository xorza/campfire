fn on_interval(ctx, m) {
    ctx.damage(m.source, m.carrier, m.carrier.max_health * ctx.value("burn_share"), "magic");
}
