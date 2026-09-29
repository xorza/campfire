fn on_cast(ctx, caster, target) {
    ctx.projectile_line(caster, caster.pos, caster.pos.direction_to(target), #{
        speed: ctx.value("speed"),
        range: ctx.range,
        width: ctx.value("width"),
        stop_on_hit: true,
        hits: "enemy_heroes",
    });
}

fn on_projectile_hit(ctx, proj, target) {
    let reach = (proj.distance / ctx.value("full_stun_distance")).min(1);
    let stun = ctx.value("min_stun_ms") + (ctx.value("max_stun_ms") - ctx.value("min_stun_ms")) * reach;
    ctx.damage(proj.source, target, ctx.value("damage"), "magic");
    ctx.stun(target, stun.round());
    for other in ctx.enemies_near(proj.source, target.pos, ctx.value("splash_radius")) {
        if other != target {
            ctx.damage(proj.source, other, ctx.value("damage") * ctx.value("splash_share"), "magic");
        }
        ctx.slow(other, ctx.value("slow"), ctx.value("slow_ms"));
    }
}
