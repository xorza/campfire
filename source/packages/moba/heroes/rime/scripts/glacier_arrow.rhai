fn on_resolve(ctx, caster, target) {
    ctx.projectile(caster.pos, caster.pos.direction_to(target));
}

fn on_hit(ctx, caster, target, hit) {
    let reach = (hit.distance / ctx.p.full_stun_distance).min(1);
    let stun = ctx.p.min_stun_ms + (ctx.p.max_stun_ms - ctx.p.min_stun_ms) * reach;
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "stun", stun.round());
    for other in ctx.find(caster, target.pos, ctx.p.splash_radius, "enemies") {
        if other != target {
            ctx.damage(other, ctx.p.damage * ctx.p.splash_share, "magic");
        }
        ctx.add_modifier(other, "slow", ctx.p.slow_ms);
    }
}
