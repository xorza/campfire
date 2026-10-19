fn on_hit(ctx, caster, target, hit) {
    let reach = (hit.distance / ctx.params.full_stun_distance).min(1);
    let stun = ctx.params.min_stun_ms + (ctx.params.max_stun_ms - ctx.params.min_stun_ms) * reach;
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "stun", stun.round());
    for other in ctx.find(caster, target.pos, ctx.params.splash_radius, "enemies") {
        if other != target {
            ctx.damage(other, "magic", ctx.params.damage * ctx.params.splash_share);
        }
        ctx.add_modifier(other, "slow", ctx.params.slow_ms);
    }
}
