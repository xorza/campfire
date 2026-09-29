fn on_cast(ctx, caster, target) {
    ctx.projectile_line(caster, caster.pos, caster.pos.direction_to(target), #{
        speed: ctx.value("speed"),
        range: ctx.range,
        width: ctx.value("width"),
        stop_on_hit: true,
    });
}

fn on_projectile_hit(ctx, proj, target) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(proj.source, target, ctx.value("damage"), "magic");
    if ablaze {
        ctx.stun(target, ctx.value("stun_ms"));
    }
    ctx.add_modifier(target, "kindle");
}
