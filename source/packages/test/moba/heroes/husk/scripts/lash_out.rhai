fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.params.radius, "enemies") {
        ctx.damage(unit, "magic", ctx.params.damage);
    }
}

fn on_damage_taken(ctx, m, d) {
    if d.is_attack {
        ctx.cut_cooldown(m.carrier, "lash_out", ctx.params.cooldown_cut_ms);
    }
}
