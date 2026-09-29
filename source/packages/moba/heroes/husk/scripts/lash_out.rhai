fn on_cast(ctx, caster, target) {
    for unit in ctx.enemies_near(caster, caster.pos, ctx.value("radius")) {
        ctx.damage(caster, unit, ctx.value("damage"), "magic");
    }
}

fn on_damage_taken(ctx, m, source, dmg) {
    if dmg.attack {
        ctx.reduce_cooldown(m.carrier, "lash_out", ctx.value("cooldown_cut_ms"));
    }
}
