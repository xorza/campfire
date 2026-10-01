fn on_resolve(ctx, caster, target) {
    ctx.projectile(caster.pos, caster.pos.direction_to(target));
}

fn on_hit(ctx, caster, target, hit) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(target, ctx.p.damage, "magic");
    if ablaze {
        ctx.add_modifier(target, "stun", ctx.p.stun_ms);
    }
    ctx.add_modifier(target, "kindle");
}
