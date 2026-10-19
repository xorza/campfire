fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, "magic", ctx.params.damage);
    ctx.add_modifier(target, "stun", ctx.params.stun_ms);
    ctx.dash(caster, target, ctx.params.pull_speed);
}
