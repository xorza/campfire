fn on_cast(ctx, caster, target) {
    ctx.area(target);
}

fn on_area_trigger(ctx, area, units) {
    for unit in units {
        let damage = ctx.p.damage;
        if unit.has_modifier("kindle") {
            damage *= ctx.p.ablaze_bonus;
        }
        ctx.damage(unit, damage, "magic");
        ctx.add_modifier(unit, "kindle");
    }
}
