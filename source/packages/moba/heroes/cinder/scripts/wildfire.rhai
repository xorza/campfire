fn on_resolve(ctx, caster, target) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "kindle");
    if !ablaze {
        return;
    }
    for unit in ctx.find(caster, target.pos, ctx.p.spread_radius, "enemies") {
        if unit != target {
            ctx.damage(unit, ctx.p.spread_damage, "magic");
            ctx.add_modifier(unit, "kindle");
        }
    }
}
