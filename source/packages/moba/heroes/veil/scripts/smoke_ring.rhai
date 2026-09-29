fn on_cast(ctx, caster, target) {
    ctx.area_circle(caster, caster.pos, #{
        radius: ctx.value("radius"),
        duration_ms: ctx.value("duration_ms"),
        interval_ms: ctx.value("tick_ms"),
        affects: "all",
    });
}

// Effects last a little longer than one tick, so they hold without gaps while a unit stays inside.
fn on_area_tick(ctx, area, units) {
    for unit in units {
        if unit == area.source {
            ctx.add_modifier(unit, "smoke_ring_cover", ctx.value("refresh_ms"));
        } else if unit.is_enemy_of(area.source) {
            ctx.slow(unit, ctx.value("slow"), ctx.value("refresh_ms"));
        }
    }
}
