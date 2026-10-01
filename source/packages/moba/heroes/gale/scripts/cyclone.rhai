// A charged cast: `ctx.origin` and `target` are where the charge began, since Gale may move while it grows.
fn on_resolve(ctx, caster, target) {
    let range = ctx.p.range_min + (ctx.range - ctx.p.range_min) * ctx.charge;
    let tornado = ctx.projectile(ctx.origin, ctx.origin.direction_to(target), #{ range: range });
    tornado.state.charge = ctx.charge;
}

fn on_hit(ctx, caster, target, hit) {
    let charge = hit.delivery.state.charge;
    let up = ctx.p.knock_up_min_ms + (ctx.p.knock_up_max_ms - ctx.p.knock_up_min_ms) * charge;
    ctx.damage(target, ctx.p.damage + ctx.p.charge_damage * charge, "magic");
    ctx.add_modifier(target, "knock_up", up.round());
}
