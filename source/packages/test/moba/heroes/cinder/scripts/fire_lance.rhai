fn on_hit(ctx, caster, target, hit) {
    let ablaze = target.has_modifier("kindle");
    ctx.damage(target, "magic", ctx.params.damage);
    if ablaze {
        ctx.add_modifier(target, "stun", ctx.params.stun_ms);
    }
    ctx.add_modifier(target, "kindle");
}
