fn on_attack_hit(ctx, m, d) {
    ctx.slow(d.target, ctx.p.slow, ctx.p.slow_ms);
}
