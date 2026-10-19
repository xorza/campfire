fn on_end(ctx, caster, hit) {
    ctx.reveal(hit.pos, ctx.params.sight_radius, ctx.params.linger_ms);
}

fn on_kill(ctx, m, victim) {
    ctx.add_resource(m.carrier.owner, "gold", ctx.params.bonus_gold);
}
