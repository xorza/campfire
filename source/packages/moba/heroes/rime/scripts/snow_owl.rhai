fn on_end(ctx, caster, hit) {
    ctx.reveal(hit.pos, ctx.p.sight_radius, ctx.p.linger_ms);
}

fn on_kill(ctx, m, victim) {
    ctx.add_resource(m.carrier.owner, "gold", ctx.p.bonus_gold);
}
