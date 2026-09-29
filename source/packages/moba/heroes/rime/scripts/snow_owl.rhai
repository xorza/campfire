fn on_cast(ctx, caster, target) {
    ctx.projectile_to(caster, caster.pos, target, #{
        speed: ctx.value("speed"),
        sight_radius: ctx.value("sight_radius"),
        collide: false,
    });
}

fn on_projectile_end(ctx, proj) {
    ctx.reveal(proj.source, proj.pos, ctx.value("sight_radius"), ctx.value("linger_ms"));
}

fn on_kill(ctx, m, victim) {
    ctx.add_resource(m.carrier.owner, "gold", ctx.value("bonus_gold"));
}
