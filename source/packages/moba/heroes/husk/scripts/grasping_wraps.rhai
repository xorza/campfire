fn on_resolve(ctx, caster, target) {
    ctx.projectile(caster.pos, caster.pos.direction_to(target));
}

fn on_hit(ctx, caster, target, hit) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.stun(target, ctx.p.stun_ms);
    ctx.dash(caster, target, ctx.p.pull_speed);
}
