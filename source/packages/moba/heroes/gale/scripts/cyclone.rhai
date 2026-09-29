fn on_cast(ctx, caster, target) {
    caster.state.cyclone_start_ms = ctx.now_ms();
    caster.state.cyclone_from = caster.pos;
    caster.state.cyclone_aim = caster.pos.direction_to(target);
}

// Recast and the end of the recast window release the tornado the same way.
fn on_recast(ctx, caster, timed_out) {
    let held = (ctx.now_ms() - caster.state.cyclone_start_ms).min(ctx.value("full_charge_ms"));
    let charge = num(held) / ctx.value("full_charge_ms");
    let range = ctx.value("range_min") + (ctx.range - ctx.value("range_min")) * charge;
    let tornado = ctx.projectile_line(caster, caster.state.cyclone_from, caster.state.cyclone_aim, #{
        speed: ctx.value("speed"),
        range: range,
        width: ctx.value("width"),
        stop_on_hit: false,
    });
    tornado.state.charge = charge;
}

fn on_projectile_hit(ctx, proj, target) {
    let charge = proj.state.charge;
    let up = ctx.value("knock_up_min_ms") + (ctx.value("knock_up_max_ms") - ctx.value("knock_up_min_ms")) * charge;
    ctx.damage(proj.source, target, ctx.value("damage") + ctx.value("charge_damage") * charge, "magic");
    ctx.knock_up(target, up.round());
}
