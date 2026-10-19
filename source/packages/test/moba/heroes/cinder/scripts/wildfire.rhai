fn on_resolve(ctx, caster, target) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "kindle");
    if !ablaze {
        return;
    }
    for unit in ctx.find(caster, target.position, ctx.params.spread_radius, "enemies") {
        if unit != target {
            ctx.damage(unit, "magic", ctx.params.spread_damage);
            ctx.add_modifier(unit, "kindle");
        }
    }
}
