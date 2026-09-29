fn on_cast(ctx, caster, target) {
    ctx.dash(caster, target, ctx.p.speed);
}

fn on_dash_end(ctx, caster, target) {
    ctx.damage(target, ctx.p.damage, "magic");
}

fn on_takedown(ctx, m, victim) {
    if victim.is_hero {
        ctx.add_charge(m.carrier, "night_step");
    }
}
