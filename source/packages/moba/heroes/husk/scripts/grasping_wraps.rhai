fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.add_modifier(target, "stun", ctx.p.stun_ms);
    ctx.dash(caster, target, ctx.p.pull_speed);
}
