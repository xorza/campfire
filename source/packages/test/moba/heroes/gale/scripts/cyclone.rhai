fn on_hit(ctx, caster, target, hit) {
    let charge = ctx.charged;
    let up = ctx.params.knock_up_min_ms + (ctx.params.knock_up_max_ms - ctx.params.knock_up_min_ms) * charge;
    ctx.damage(target, "magic", ctx.params.damage + ctx.params.charge_damage * charge);
    ctx.add_modifier(target, "knock_up", up.round());
}
