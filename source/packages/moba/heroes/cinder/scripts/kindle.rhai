fn on_interval(ctx, m) {
    ctx.damage(m.carrier, m.carrier.max_health * ctx.p.burn_share, "magic");
}
