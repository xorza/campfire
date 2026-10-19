fn on_resolve(ctx, caster, target) {
    ctx.dash(caster, target, ctx.params.speed);
}

fn on_end(ctx, caster, hit) {
    ctx.damage(hit.aimed, "magic", ctx.params.damage);
}

fn on_takedown(ctx, m, victim) {
    if victim.is_avatar {
        ctx.add_charge(m.carrier, "night_step");
    }
}
