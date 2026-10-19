fn on_interval(ctx, m) {
    ctx.damage(m.carrier, "magic", m.carrier.pool_max("health") * ctx.params.burn_share);
}
