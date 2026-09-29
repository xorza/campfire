fn on_cast(ctx, caster, target) {
    ctx.projectile(caster.pos, target);
}

fn on_projectile_end(ctx, proj) {
    ctx.reveal(proj.pos, ctx.p.sight_radius, ctx.p.linger_ms);
}

fn on_kill(ctx, m, victim) {
    ctx.add_resource(m.carrier.owner, "gold", ctx.p.bonus_gold);
}
