fn on_cast(ctx, caster, target) {
    ctx.area_circle(caster, target, #{
        radius: ctx.value("radius"),
        delay_ms: ctx.value("delay_ms"),
        affects: "enemies",
    });
}

fn on_area_trigger(ctx, area, units) {
    for unit in units {
        let damage = ctx.value("damage");
        if unit.has_modifier("kindle") {
            damage *= ctx.value("ablaze_bonus");
        }
        ctx.damage(area.source, unit, damage, "magic");
        ctx.add_modifier(unit, "kindle");
    }
}
