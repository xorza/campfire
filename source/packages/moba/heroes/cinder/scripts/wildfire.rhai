fn on_cast(ctx, caster, target) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(caster, target, ctx.value("damage"), "magic");
    ctx.add_modifier(target, "kindle");
    if !ablaze {
        return;
    }
    for unit in ctx.enemies_near(caster, target.pos, ctx.value("spread_radius")) {
        if unit != target {
            ctx.damage(caster, unit, ctx.value("spread_damage"), "magic");
            ctx.add_modifier(unit, "kindle");
        }
    }
}
