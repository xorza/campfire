fn on_resolve(ctx, caster, target) {
    ctx.dash(caster, target, ctx.p.speed);
}

fn on_end(ctx, caster, hit) {
    ctx.damage(hit.target, ctx.p.damage, "magic");
}

fn on_takedown(ctx, m, victim) {
    if victim.is_avatar {
        ctx.add_charge(m.carrier, "night_step");
    }
}
