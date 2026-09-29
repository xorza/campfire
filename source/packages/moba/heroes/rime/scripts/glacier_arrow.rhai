fn on_cast(ctx, caster, target) {
    ctx.projectile(caster.pos, caster.pos.direction_to(target));
}

fn on_projectile_hit(ctx, proj, target) {
    let reach = (proj.distance / ctx.p.full_stun_distance).min(1);
    let stun = ctx.p.min_stun_ms + (ctx.p.max_stun_ms - ctx.p.min_stun_ms) * reach;
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.stun(target, stun.round());
    for other in ctx.find(proj.source, target.pos, ctx.p.splash_radius, "enemies") {
        if other != target {
            ctx.damage(other, ctx.p.damage * ctx.p.splash_share, "magic");
        }
        ctx.slow(other, ctx.p.slow, ctx.p.slow_ms);
    }
}
