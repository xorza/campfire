fn on_hit(ctx, caster, target, hit) {
    let charge = ctx.charge;
    let up = ctx.p.knock_up_min_ms + (ctx.p.knock_up_max_ms - ctx.p.knock_up_min_ms) * charge;
    ctx.damage(target, ctx.p.damage + ctx.p.charge_damage * charge, "magic");
    ctx.add_modifier(target, "knock_up", up.round());
}
