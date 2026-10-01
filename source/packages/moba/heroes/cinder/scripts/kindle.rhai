fn on_interval(ctx, m) {
    ctx.damage(m.carrier, m.carrier.pool_max("health") * ctx.p.burn_share, "magic");
}
