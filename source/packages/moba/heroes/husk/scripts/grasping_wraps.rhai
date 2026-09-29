fn on_cast(ctx, caster, target) {
    ctx.projectile(caster.pos, caster.pos.direction_to(target));
}

fn on_projectile_hit(ctx, proj, target) {
    ctx.damage(target, ctx.p.damage, "magic");
    ctx.stun(target, ctx.p.stun_ms);
    ctx.dash(proj.source, target, ctx.p.pull_speed);
}
